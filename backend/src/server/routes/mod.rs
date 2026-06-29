use std::time::Duration;

use axum::{
    Router,
    http::{HeaderValue, Method, Request, Response},
    response::IntoResponse,
    routing::get,
};
use axum_client_ip::ClientIpSource;
use reqwest::{StatusCode, header};
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::CorsLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    set_header::{SetRequestHeaderLayer, SetResponseHeaderLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::{Span, debug, field, info};

use crate::server::error::ApiError;

mod frontend;
pub mod v1;
mod well_known;

#[derive(Clone)]
struct MakeRequestUlid;
impl MakeRequestId for MakeRequestUlid {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let mut id = ulid::Ulid::new().to_string();
        id.make_ascii_lowercase();
        let val = HeaderValue::from_str(&id).ok()?;

        Some(RequestId::new(val))
    }
}

fn redact_uri(uri: &axum::http::Uri) -> String {
    if uri.path().starts_with("/api/v1/auth/") {
        return uri.path().to_string();
    }
    let Some(query) = uri.query() else {
        return uri.to_string();
    };
    let masked = query
        .split('&')
        .map(|pair| {
            let key = pair.split_once('=').map_or(pair, |(k, _)| k);
            if matches!(key, "token" | "ticket") {
                format!("{key}=REDACTED")
            } else {
                pair.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("&");
    let mut s = uri.to_string();
    if let Some(idx) = s.find('?') {
        s.truncate(idx);
        s.push('?');
        s.push_str(&masked);
    }
    s
}

pub fn create_router(ip_source: ClientIpSource) -> Router {
    let auth_callback = Router::new().route(
        "/api/v1/auth/{provider}/callback",
        get(v1::auth::callback).layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        )),
    );

    let rest = Router::new()
        .fallback_service(frontend::create_service())
        .route(
            "/.well-known/microsoft-identity-association.json",
            get(well_known::microsoft_identity_association_json),
        )
        .nest("/api/v1", v1::create_v1_router())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(10),
        ));

    add_middlewares(auth_callback.merge(rest), ip_source)
}

#[allow(clippy::too_many_lines)]
fn add_middlewares<T>(router: Router<T>, ip_source: ClientIpSource) -> Router<T>
where
    T: std::clone::Clone + Send + Sync + 'static,
{
    router
        .layer(CatchPanicLayer::custom(|err| {
            debug!(?err, "Panic caught in request handling");

            ApiError::internal(format!("Internal Server Error: {err:?}")).into_response()
        }))
        .layer(
            ServiceBuilder::new()
                .layer(ip_source.into_extension())
                .layer(SetRequestHeaderLayer::if_not_present(
                    "x-forwarded-for"
                        .parse()
                        .expect("Header name should be valid"),
                    |request: &Request<_>| {
                        let connect_info = request
                            .extensions()
                            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()?;

                        connect_info.ip().to_string().parse().ok()
                    },
                ))
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUlid))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &Request<_>| {
                            let m = request.method();
                            let p = request.uri().path();
                            let id = request
                                .extensions()
                                .get::<RequestId>()
                                .and_then(|id| id.header_value().to_str().ok())
                                .unwrap_or("-");
                            let dur = field::Empty;
                            let user = field::Empty;

                            tracing::info_span!("", %id, %m, ?p, dur, user)
                        })
                        .on_request(|request: &Request<_>, _span: &Span| {
                            let headers = request.headers();
                            info!(
                                target: "request",
                                "START \"{method} {uri} {http_type:?}\" {user_agent:?} {ip:?}",
                                http_type = request.version(),
                                method = request.method(),
                                uri = redact_uri(request.uri()),
                                user_agent = headers
                                    .get(header::USER_AGENT)
                                    .map_or("-", |x| x.to_str().unwrap_or("-")),
                                ip = headers
                                    .get("x-forwarded-for")
                                    .map_or("-", |x| x.to_str().unwrap_or("-")),
                            );
                        })
                        .on_response(|response: &Response<_>, latency, span: &Span| {
                            span.record("dur", field::debug(latency));
                            info!(
                                target: "request",
                                "END {status}",
                                status = response.status().as_u16(),
                            );
                        })
                        .on_body_chunk(())
                        .on_eos(|_trailers: Option<&_>, stream_duration, span: &Span| {
                            span.record("dur", field::debug(stream_duration));
                            debug!(
                                target: "request",
                                "ERR: stream closed unexpectedly",
                            );
                        })
                        .on_failure(|error, latency, span: &Span| {
                            span.record("dur", field::debug(latency));
                            debug!(
                                target: "request",
                                err = ?error,
                                "ERR: something went wrong",
                            );
                        }),
                )
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(SetResponseHeaderLayer::appending(
                    header::DATE,
                    |_response: &Response<_>| {
                        Some(
                            jiff::fmt::rfc2822::to_string(&jiff::Zoned::now())
                                .expect("current time should be formattable as RFC 2822")
                                .parse()
                                .expect("RFC 2822 string should be a valid HeaderValue"),
                        )
                    },
                )),
        )
        .layer({
            let origins = crate::auth::config::get().allowed_origins.clone();
            if origins.is_empty() {
                CorsLayer::very_permissive()
            } else {
                let allowed = origins
                    .into_iter()
                    .filter_map(|o| HeaderValue::from_str(&o).ok())
                    .collect::<Vec<_>>();

                CorsLayer::new()
                    .allow_origin(allowed)
                    .allow_methods([
                        Method::GET,
                        Method::POST,
                        Method::PUT,
                        Method::DELETE,
                        Method::OPTIONS,
                    ])
                    .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
            }
        })
}
