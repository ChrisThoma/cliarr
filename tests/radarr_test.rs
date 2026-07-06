use cliarr::api::http::build_client;
use cliarr::api::radarr::RadarrClient;
use cliarr::config::ApiKeyService;
use cliarr::error::CliarrError;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn client(server: &MockServer) -> RadarrClient {
    client_with_base(&server.uri())
}

fn client_with_base(base: &str) -> RadarrClient {
    RadarrClient::new(
        &ApiKeyService {
            url: base.to_string(),
            api_key: "test-key".into(),
        },
        build_client(),
    )
}

fn lookup_fixture() -> serde_json::Value {
    json!([{
        "title": "Dune: Part Two",
        "year": 2024,
        "tmdbId": 693134,
        "imdbId": "tt15239678",
        "overview": "Follow the mythic journey of Paul Atreides.",
        "status": "released",
        "runtime": 167,
        "genres": ["Science Fiction", "Adventure"],
        "images": [
            {"coverType": "poster", "remoteUrl": "https://image.tmdb.org/t/p/original/dune2.jpg"}
        ]
    }])
}

#[tokio::test]
async fn lookup_sends_key_and_deserializes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/movie/lookup"))
        .and(query_param("term", "dune"))
        .and(header("X-Api-Key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(lookup_fixture()))
        .expect(1)
        .mount(&server)
        .await;

    let movies = client(&server).lookup("dune").await.unwrap();
    assert_eq!(movies.len(), 1);
    let m = &movies[0];
    assert_eq!(m.tmdb_id, 693134);
    assert_eq!(m.id, 0, "lookup results not in library default to id 0");
    assert_eq!(m.poster_remote_url(), Some("https://image.tmdb.org/t/p/original/dune2.jpg"));
}

#[tokio::test]
async fn base_path_prefix_is_preserved() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/radarr/api/v3/system/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"version": "5.9.1"})))
        .expect(1)
        .mount(&server)
        .await;

    let status = client_with_base(&format!("{}/radarr", server.uri()))
        .system_status()
        .await
        .unwrap();
    assert_eq!(status.version, "5.9.1");
}

#[tokio::test]
async fn add_movie_posts_expected_payload() {
    let server = MockServer::start().await;
    let movie: cliarr::api::models::radarr::Movie =
        serde_json::from_value(lookup_fixture()[0].clone()).unwrap();

    Mock::given(method("POST"))
        .and(path("/api/v3/movie"))
        .and(body_partial_json(json!({
            "title": "Dune: Part Two",
            "tmdbId": 693134,
            "qualityProfileId": 4,
            "rootFolderPath": "/volume1/movies",
            "monitored": true,
            "addOptions": {"searchForMovie": true}
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": 42, "title": "Dune: Part Two", "year": 2024, "tmdbId": 693134
        })))
        .expect(1)
        .mount(&server)
        .await;

    let added = client(&server)
        .add_movie(&movie, 4, "/volume1/movies", true, true)
        .await
        .unwrap();
    assert_eq!(added.id, 42);
}

#[tokio::test]
async fn edit_movie_puts_partial_update_to_editor() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v3/movie/editor"))
        .and(header("X-Api-Key", "test-key"))
        .and(body_partial_json(json!({
            "movieIds": [42],
            "qualityProfileId": 7,
            "monitored": false
        })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).edit_movie(42, 7, false).await.unwrap();
}

#[tokio::test]
async fn unauthorized_maps_to_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/system/status"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let err = client(&server).system_status().await.unwrap_err();
    assert!(matches!(err, CliarrError::Auth { service: "radarr" }), "got: {err:?}");
}

#[tokio::test]
async fn queue_unwraps_paged_records() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/queue"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "page": 1, "pageSize": 200, "totalRecords": 1,
            "records": [{
                "id": 7, "title": "Dune.Part.Two.2024.1080p", "status": "downloading",
                "timeleft": "00:12:00", "size": 8e9, "sizeleft": 2e9,
                "protocol": "torrent", "downloadClient": "qBittorrent", "movieId": 42
            }]
        })))
        .mount(&server)
        .await;

    let queue = client(&server).queue().await.unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].progress().unwrap().round(), 75.0);
}

#[tokio::test]
async fn queue_delete_sends_blocklist_flags() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/api/v3/queue/7"))
        .and(query_param("blocklist", "true"))
        .and(query_param("removeFromClient", "true"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    client(&server).queue_delete(7, true, true).await.unwrap();
}
