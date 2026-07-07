use cliarr::api::http::build_client;
use cliarr::api::sonarr::SonarrClient;
use cliarr::config::ApiKeyService;
use cliarr::error::CliarrError;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> SonarrClient {
    SonarrClient::new(
        &ApiKeyService {
            url: server.uri(),
            api_key: "test-key".into(),
        },
        build_client(),
    )
}

#[tokio::test]
async fn edit_series_puts_partial_update_to_editor() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v3/series/editor"))
        .and(header("X-Api-Key", "test-key"))
        .and(body_partial_json(json!({
            "seriesIds": [7],
            "qualityProfileId": 3,
            "monitored": true
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).edit_series(7, 3, true).await.unwrap();
}

#[tokio::test]
async fn lookup_deserializes_series() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/series/lookup"))
        .and(query_param("term", "severance"))
        .and(header("X-Api-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "title": "Severance",
            "year": 2022,
            "tvdbId": 371980,
            "status": "continuing",
            "network": "Apple TV+",
            "seasons": [{"seasonNumber": 1, "monitored": true}],
            "images": [{"coverType": "poster", "remoteUrl": "https://artworks.thetvdb.com/severance.jpg"}]
        }])))
        .mount(&server)
        .await;

    let results = client(&server).lookup("severance").await.unwrap();
    assert_eq!(results[0].tvdb_id, 371980);
    assert_eq!(results[0].seasons.len(), 1);
}

#[tokio::test]
async fn add_series_posts_expected_payload() {
    let server = MockServer::start().await;
    let series: cliarr::api::models::sonarr::Series = serde_json::from_value(json!({
        "title": "Severance", "tvdbId": 371980,
        "seasons": [{"seasonNumber": 1, "monitored": true}]
    }))
    .unwrap();

    Mock::given(method("POST"))
        .and(path("/api/v3/series"))
        .and(body_partial_json(json!({
            "title": "Severance",
            "tvdbId": 371980,
            "qualityProfileId": 6,
            "rootFolderPath": "/volume1/tv",
            "monitored": true,
            "seasonFolder": true,
            "addOptions": {"searchForMissingEpisodes": true, "monitor": "all"}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 9, "title": "Severance", "tvdbId": 371980
        })))
        .expect(1)
        .mount(&server)
        .await;

    let added = client(&server)
        .add_series(&series, 6, "/volume1/tv", true, true, true)
        .await
        .unwrap();
    assert_eq!(added.id, 9);
}

// The next five tests exercise the endpoints shared with Radarr, which live
// on ArrCore and are reached through SonarrClient's Deref.

#[tokio::test]
async fn system_status_reaches_shared_core() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/system/status"))
        .and(header("X-Api-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "4.0.10"})))
        .expect(1)
        .mount(&server)
        .await;

    let status = client(&server).system_status().await.unwrap();
    assert_eq!(status.version, "4.0.10");
}

#[tokio::test]
async fn unauthorized_maps_to_sonarr_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/system/status"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = client(&server).system_status().await.unwrap_err();
    assert!(
        matches!(err, CliarrError::Auth { service: "sonarr" }),
        "shared core must keep the per-service name; got: {err:?}"
    );
}

#[tokio::test]
async fn queue_delete_sends_blocklist_flags() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v3/queue/9"))
        .and(query_param("blocklist", "false"))
        .and(query_param("removeFromClient", "true"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).queue_delete(9, false, true).await.unwrap();
}

#[tokio::test]
async fn command_merges_extra_fields_into_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v3/command"))
        .and(header("X-Api-Key", "test-key"))
        .and(body_partial_json(json!({"name": "SeriesSearch", "seriesId": 9})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 1})))
        .expect(1)
        .mount(&server)
        .await;

    client(&server)
        .command("SeriesSearch", json!({"seriesId": 9}))
        .await
        .unwrap();
}

#[tokio::test]
async fn quality_profiles_and_root_folders_deserialize() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/qualityprofile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 6, "name": "HD-1080p"}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/rootfolder"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "path": "/volume1/tv", "freeSpace": 5_000_000_000i64}
        ])))
        .mount(&server)
        .await;

    let c = client(&server);
    let profiles = c.quality_profiles().await.unwrap();
    assert_eq!(profiles[0].name, "HD-1080p");
    let roots = c.root_folders().await.unwrap();
    assert_eq!(roots[0].path, "/volume1/tv");
}

#[tokio::test]
async fn missing_includes_series_and_unwraps_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/wanted/missing"))
        .and(query_param("includeSeries", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "page": 1, "pageSize": 100, "totalRecords": 1,
            "records": [{
                "id": 100, "seriesId": 9, "seasonNumber": 2, "episodeNumber": 3,
                "title": "Who Is Alive?", "airDateUtc": "2025-01-31T02:00:00Z",
                "monitored": true,
                "series": {"title": "Severance", "tvdbId": 371980}
            }]
        })))
        .mount(&server)
        .await;

    let missing = client(&server).missing().await.unwrap();
    let ep = &missing.records[0];
    assert_eq!(ep.code(), "S02E03");
    assert_eq!(ep.series_title(), "Severance");
}
