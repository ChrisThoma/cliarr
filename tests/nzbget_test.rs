use cliarr::api::http::build_client;
use cliarr::api::nzbget::NzbgetClient;
use cliarr::config::UserPassService;
use cliarr::error::CliarrError;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> NzbgetClient {
    NzbgetClient::new(
        &UserPassService {
            url: server.uri(),
            username: "nzbget".into(),
            password: "tegbzn6789".into(),
        },
        build_client(),
    )
}

#[tokio::test]
async fn listgroups_parses_and_uses_basic_auth() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/jsonrpc"))
        .and(header_exists("authorization"))
        .and(body_partial_json(json!({"method": "listgroups"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": "1.1",
            "result": [{
                "NZBID": 21,
                "NZBName": "Severance.S02E03.1080p.WEB",
                "Status": "DOWNLOADING",
                "FileSizeMB": 3000,
                "RemainingSizeMB": 750,
                "DownloadedSizeMB": 2250,
                "Category": "tv"
            }]
        })))
        .mount(&server)
        .await;

    let groups = client(&server).listgroups().await.unwrap();
    assert_eq!(groups[0].id, 21);
    assert_eq!(groups[0].progress().round(), 75.0);
}

#[tokio::test]
async fn rpc_error_member_becomes_api_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/jsonrpc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "version": "1.1",
            "error": {"name": "InvalidParam", "code": -32602, "message": "bad params"},
            "result": null
        })))
        .mount(&server)
        .await;

    let err = client(&server).listgroups().await.unwrap_err();
    assert!(matches!(err, CliarrError::Api { service: "nzbget", .. }), "got: {err:?}");
}

#[tokio::test]
async fn edit_queue_sends_group_command_shape() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/jsonrpc"))
        .and(body_partial_json(json!({
            "method": "editqueue",
            "params": ["GroupPause", "", [21, 22]]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "1.1", "result": true})))
        .expect(1)
        .mount(&server)
        .await;

    assert!(client(&server).pause(&[21, 22]).await.unwrap());
}
