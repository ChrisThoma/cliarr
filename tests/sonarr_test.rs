use cliarr::api::http::build_client;
use cliarr::api::sonarr::SonarrClient;
use cliarr::config::ApiKeyService;
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
