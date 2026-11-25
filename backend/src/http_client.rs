use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::Duration,
};

use axum::http::HeaderValue;
use reqwest::{StatusCode, header::RETRY_AFTER};
use tokio::time::Instant;
use tracing::{debug, warn};
use url::Origin;

const MAX_RETRY_AFTER: Duration = Duration::from_mins(2);

static BLOCKED_UNTIL: LazyLock<Mutex<HashMap<Origin, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    client_builder()
        .build()
        .expect("Failed to build HTTP client")
});

pub static OAUTH_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    client_builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build OAuth HTTP client")
});

fn client_builder() -> reqwest::ClientBuilder {
    let certs = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|cert| reqwest::Certificate::from_der(cert.as_ref()).ok());

    reqwest::Client::builder()
        .tls_certs_only(certs)
        .redirect(reqwest::redirect::Policy::limited(3))
}

pub async fn send_with_backoff(
    request: reqwest::RequestBuilder,
) -> reqwest::Result<reqwest::Response> {
    let (client, request) = request.build_split();
    let request = request?;
    let origin = request.url().origin();

    let blocked_until = BLOCKED_UNTIL
        .lock()
        .expect("backoff lock is not poisoned")
        .get(&origin)
        .copied();
    if let Some(until) = blocked_until
        && until > Instant::now()
    {
        debug!(origin = %origin.ascii_serialization(), wait = ?(until - Instant::now()), "Waiting for Retry-After delay");
        tokio::time::sleep_until(until).await;
    }

    let response = client.execute(request).await?;

    if matches!(
        response.status(),
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE
    ) {
        let delay = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|x| parse_retry_after(x, jiff::Timestamp::now()));

        if let Some(delay) = delay {
            warn!(origin = %origin.ascii_serialization(), status = %response.status(), ?delay, "Server asked to retry later");
            BLOCKED_UNTIL
                .lock()
                .expect("backoff lock is not poisoned")
                .insert(origin, Instant::now() + delay);
        }
    }

    Ok(response)
}

fn parse_retry_after(value: &HeaderValue, now: jiff::Timestamp) -> Option<Duration> {
    let value = value.to_str().ok()?.trim();

    let delay = if let Ok(seconds) = value.parse::<u64>() {
        Duration::from_secs(seconds)
    } else {
        let date = jiff::fmt::rfc2822::parse(value).ok()?.timestamp();
        now.duration_until(date)
            .try_into()
            .unwrap_or(Duration::ZERO)
    };

    Some(delay.min(MAX_RETRY_AFTER))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Router, response::IntoResponse, routing::get};

    use super::*;

    #[test]
    fn parses_retry_after_forms() {
        let now = "2026-11-25T10:34:16Z"
            .parse::<jiff::Timestamp>()
            .expect("test timestamp is valid");

        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("10"), now),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            parse_retry_after(
                &HeaderValue::from_static("Wed, 25 Nov 2026 10:34:46 GMT"),
                now
            ),
            Some(Duration::from_secs(30))
        );
        assert_eq!(
            parse_retry_after(
                &HeaderValue::from_static("Wed, 25 Nov 2026 10:00:00 GMT"),
                now
            ),
            Some(Duration::ZERO)
        );
        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("99999"), now),
            Some(MAX_RETRY_AFTER)
        );
        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("-1"), now),
            None
        );
        assert_eq!(
            parse_retry_after(&HeaderValue::from_static("soon"), now),
            None
        );
    }

    /// The peer rejects the first request with `Retry-After: 1`. The next
    /// request to that origin must arrive only after the delay.
    #[tokio::test]
    async fn waits_for_retry_after_before_next_request() {
        let calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new().route(
            "/",
            get({
                let calls = Arc::clone(&calls);
                async move || {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        (StatusCode::TOO_MANY_REQUESTS, [(RETRY_AFTER, "1")]).into_response()
                    } else {
                        StatusCode::OK.into_response()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener binds");
        let url = format!(
            "http://{}/",
            listener.local_addr().expect("listener has an address")
        );
        tokio::spawn(async move { axum::serve(listener, app).await });

        let client = reqwest::Client::new();
        let first = send_with_backoff(client.get(&url))
            .await
            .expect("peer responds");
        assert_eq!(first.status(), StatusCode::TOO_MANY_REQUESTS);

        let start = Instant::now();
        let second = send_with_backoff(client.get(&url))
            .await
            .expect("peer responds");
        assert_eq!(second.status(), StatusCode::OK);
        assert!(start.elapsed() >= Duration::from_millis(900));
    }
}
