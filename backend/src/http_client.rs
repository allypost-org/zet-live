use std::{sync::LazyLock, time::Duration};

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let certs = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|cert| reqwest::Certificate::from_der(cert.as_ref()).ok());

    reqwest::Client::builder()
        .tls_certs_only(certs)
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .expect("Failed to build HTTP client")
});

pub static OAUTH_HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    let certs = webpki_root_certs::TLS_SERVER_ROOT_CERTS
        .iter()
        .filter_map(|cert| reqwest::Certificate::from_der(cert.as_ref()).ok());

    reqwest::Client::builder()
        .tls_certs_only(certs)
        .redirect(reqwest::redirect::Policy::limited(3))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build OAuth HTTP client")
});
