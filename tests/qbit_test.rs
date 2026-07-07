use cliarr::api::http::build_client;
use cliarr::api::qbittorrent::QbitClient;
use cliarr::config::UserPassService;
use cliarr::error::CliarrError;
use serde_json::json;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> QbitClient {
    QbitClient::new(
        &UserPassService {
            url: server.uri(),
            username: "admin".into(),
            password: "hunter2".into(),
        },
        build_client(),
    )
}

fn torrents_fixture() -> serde_json::Value {
    json!([{
        "hash": "abcdef0123456789abcdef0123456789abcdef01",
        "name": "Dune.Part.Two.2024.1080p.BluRay",
        "state": "downloading",
        "progress": 0.42,
        "size": 8_000_000_000i64,
        "dlspeed": 5_000_000,
        "upspeed": 100_000,
        "eta": 900,
        "category": "movies",
        "num_seeds": 12,
        "ratio": 0.1
    }])
}

#[tokio::test]
async fn login_rejects_bad_credentials_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
        .mount(&server)
        .await;

    let err = client(&server).login().await.unwrap_err();
    assert!(matches!(err, CliarrError::Auth { service: "qbittorrent" }));
}

#[tokio::test]
async fn forbidden_triggers_relogin_and_retry() {
    let server = MockServer::start().await;
    // First request 403s, login succeeds, retry succeeds.
    Mock::given(method("GET"))
        .and(path("/api/v2/torrents/info"))
        .respond_with(ResponseTemplate::new(403))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .and(body_string_contains("username=admin"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/torrents/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(torrents_fixture()))
        .mount(&server)
        .await;

    let torrents = client(&server).torrents(None).await.unwrap();
    assert_eq!(torrents.len(), 1);
    assert!(!torrents[0].is_paused());
}

#[tokio::test]
async fn new_webapi_uses_stop_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/app/webapiVersion"))
        .respond_with(ResponseTemplate::new(200).set_body_string("2.11.4"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/stop"))
        .and(body_string_contains("hashes=aaa%7Cbbb"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).pause(&["aaa".into(), "bbb".into()]).await.unwrap();
}

#[tokio::test]
async fn old_webapi_uses_pause_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v2/app/webapiVersion"))
        .respond_with(ResponseTemplate::new(200).set_body_string("2.9.3"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/pause"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).pause(&["aaa".into()]).await.unwrap();
}

#[tokio::test]
async fn health_version_logs_in_then_returns_trimmed_version() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .and(body_string_contains("username=admin"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Ok."))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v2/app/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("v5.0.4\n"))
        .expect(1)
        .mount(&server)
        .await;

    let version = client(&server).health_version().await.unwrap();
    assert_eq!(version, "v5.0.4");
}

#[tokio::test]
async fn health_version_surfaces_bad_credentials() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/auth/login"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Fails."))
        .mount(&server)
        .await;
    // The version endpoint must never be hit when the login fails.
    Mock::given(method("GET"))
        .and(path("/api/v2/app/version"))
        .respond_with(ResponseTemplate::new(200).set_body_string("v5.0.4"))
        .expect(0)
        .mount(&server)
        .await;

    let err = client(&server).health_version().await.unwrap_err();
    assert!(matches!(err, CliarrError::Auth { service: "qbittorrent" }), "got: {err:?}");
}

#[tokio::test]
async fn delete_sends_delete_files_flag() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v2/torrents/delete"))
        .and(body_string_contains("deleteFiles=true"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).delete(&["aaa".into()], true).await.unwrap();
}
