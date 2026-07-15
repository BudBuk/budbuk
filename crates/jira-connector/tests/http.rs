//! HTTP integration tests for the Jira connector, using `wiremock` as a fake
//! Jira server. These cover the real client (both pagination styles, every row
//! converter, and every error path) and the connector's real-mode dispatch —
//! all without touching a real Jira instance.

use connector_sdk::{Connector, ConnectorError, Query};
use jira_connector::client::JiraClient;
use jira_connector::{JiraConfig, JiraConnector};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Build a client pointed at a mock server.
fn client_for(server: &MockServer) -> JiraClient {
    JiraClient::new(server.uri(), "e@x.com".into(), "token".into())
}

#[tokio::test]
async fn projects_paginate_by_start_at_and_convert_rows() {
    let server = MockServer::start().await;

    // Page 1 (startAt=0): a project WITH a lead, and one with NO lead and a
    // non-numeric id (exercises the id-parse fallback and the None-lead branch).
    Mock::given(method("GET"))
        .and(path("/rest/api/3/project/search"))
        .and(query_param("startAt", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "isLast": false,
            "values": [
                {"id": "1", "key": "A", "name": "Alpha", "lead": {"displayName": "Lead One"}},
                {"id": "notanumber", "key": "B", "name": "Beta"}
            ]
        })))
        .mount(&server)
        .await;
    // Page 2 (startAt=2): last page.
    Mock::given(method("GET"))
        .and(path("/rest/api/3/project/search"))
        .and(query_param("startAt", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "isLast": true,
            "values": [{"id": "3", "key": "C", "name": "Gamma", "lead": {"displayName": "Lead Two"}}]
        })))
        .mount(&server)
        .await;

    let rows = client_for(&server).projects(10).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].0[0].to_display_string(), "1"); // parsed id
    assert_eq!(rows[0].0[3].to_display_string(), "Lead One"); // lead present
    assert_eq!(rows[1].0[0].to_display_string(), "0"); // bad id -> 0
    assert_eq!(rows[1].0[3].to_display_string(), "NULL"); // lead absent
    assert_eq!(rows[2].0[1].to_display_string(), "C");
}

#[tokio::test]
async fn users_stop_on_short_page_and_convert_rows() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/users/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"accountId": "a1", "displayName": "Alice", "emailAddress": "alice@x", "active": true},
            {"accountId": "a2", "active": null}
        ])))
        .mount(&server)
        .await;

    let rows = client_for(&server).users(50).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0[2].to_display_string(), "alice@x");
    assert_eq!(rows[0].0[3].to_display_string(), "true");
    assert_eq!(rows[1].0[1].to_display_string(), "NULL"); // no displayName
    assert_eq!(rows[1].0[2].to_display_string(), "NULL"); // no email
    assert_eq!(rows[1].0[3].to_display_string(), "false"); // active null -> false
}

#[tokio::test]
async fn users_loop_continues_when_first_page_is_full() {
    let server = MockServer::start().await;

    // Page 1 returns a FULL page (50 = the offset page size), so the loop does
    // NOT stop and fetches a second, shorter page.
    let full_page: Vec<serde_json::Value> = (0..50)
        .map(|i| json!({"accountId": format!("a{i}"), "displayName": "U", "active": true}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/rest/api/3/users/search"))
        .and(query_param("startAt", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(full_page)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/users/search"))
        .and(query_param("startAt", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"accountId": "a50", "displayName": "Last", "active": true}
        ])))
        .mount(&server)
        .await;

    let rows = client_for(&server).users(100).await.unwrap();
    assert_eq!(rows.len(), 51);
}

#[tokio::test]
async fn users_stops_exactly_at_limit() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/users/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"accountId": "a", "displayName": "A", "active": true},
            {"accountId": "b", "displayName": "B", "active": true}
        ])))
        .mount(&server)
        .await;
    // limit == page contents → loop breaks on the `rows.len() >= limit` arm.
    let rows = client_for(&server).users(2).await.unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn issues_paginate_by_token_and_convert_rows() {
    let server = MockServer::start().await;

    // Page 1: full issue + a nextPageToken. Matched by its maxResults=100.
    Mock::given(method("GET"))
        .and(path("/rest/api/3/search/jql"))
        .and(query_param("maxResults", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issues": [{
                "key": "K-1",
                "fields": {
                    "summary": "Do a thing",
                    "status": {"name": "Open"},
                    "assignee": {"displayName": "Al"},
                    "project": {"key": "ENG"},
                    "created": "2026-01-01T00:00:00Z"
                }
            }],
            "nextPageToken": "t2",
            "isLast": false
        })))
        .mount(&server)
        .await;
    // Page 2: matched by the token; empty fields exercise the all-None branch.
    Mock::given(method("GET"))
        .and(path("/rest/api/3/search/jql"))
        .and(query_param("nextPageToken", "t2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issues": [{"key": "K-2", "fields": {}}],
            "isLast": true
        })))
        .mount(&server)
        .await;

    let rows = client_for(&server)
        .issues("project = ENG", 100)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0[0].to_display_string(), "K-1");
    assert_eq!(rows[0].0[2].to_display_string(), "Open");
    assert_eq!(rows[0].0[3].to_display_string(), "Al");
    assert_eq!(rows[1].0[0].to_display_string(), "K-2");
    assert_eq!(rows[1].0[1].to_display_string(), "NULL"); // summary absent
    assert_eq!(rows[1].0[5].to_display_string(), "NULL"); // created absent
}

#[tokio::test]
async fn worklogs_fetch_per_issue_and_respect_limit() {
    let server = MockServer::start().await;

    // issue_keys: return two candidate issues.
    Mock::given(method("GET"))
        .and(path("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issues": [
                {"key": "K-1", "fields": {"summary": "a"}},
                {"key": "K-2", "fields": {"summary": "b"}}
            ],
            "isLast": true
        })))
        .mount(&server)
        .await;
    // K-1 has two worklogs; one full, one with nulls + a bad id.
    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/K-1/worklog"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "worklogs": [
                {"id": "100", "author": {"displayName": "Al"}, "timeSpentSeconds": 3600, "started": "2026-01-01T00:00:00Z"},
                {"id": "bad"}
            ]
        })))
        .mount(&server)
        .await;

    // limit=2 → both worklogs from K-1 fill the limit; K-2 is never queried.
    let rows = client_for(&server)
        .worklogs("worklogDate >= -30d", 2)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0[0].to_display_string(), "100");
    assert_eq!(rows[0].0[1].to_display_string(), "K-1");
    assert_eq!(rows[0].0[2].to_display_string(), "Al");
    assert_eq!(rows[0].0[3].to_display_string(), "3600");
    assert_eq!(rows[1].0[0].to_display_string(), "0"); // bad id -> 0
    assert_eq!(rows[1].0[2].to_display_string(), "NULL"); // no author
    assert_eq!(rows[1].0[3].to_display_string(), "NULL"); // no timeSpentSeconds
}

#[tokio::test]
async fn auth_error_maps_to_auth_variant() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
        .mount(&server)
        .await;
    let err = client_for(&server).projects(5).await.unwrap_err();
    assert!(matches!(err, ConnectorError::Auth(_)));
}

#[tokio::test]
async fn server_error_maps_to_other_variant() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;
    let err = client_for(&server).users(5).await.unwrap_err();
    assert!(matches!(err, ConnectorError::Other(_)));
}

#[tokio::test]
async fn invalid_json_maps_to_parse_variant() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let err = client_for(&server).users(5).await.unwrap_err();
    assert!(matches!(err, ConnectorError::Parse(_)));
}

#[tokio::test]
async fn unreachable_host_maps_to_network_variant() {
    // Port 1 refuses connections quickly → a transport error.
    let client = JiraClient::new("http://127.0.0.1:1".into(), "e@x.com".into(), "t".into());
    let err = client.projects(5).await.unwrap_err();
    assert!(matches!(err, ConnectorError::Network(_)));
}

/// A mock server that answers every endpoint the connector dispatches to, so we
/// can exercise `JiraConnector::fetch` in REAL mode for all four tables.
async fn full_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/project/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "isLast": true,
            "values": [{"id": "1", "key": "A", "name": "Alpha", "lead": {"displayName": "L"}}]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/search/jql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "issues": [{"key": "K-1", "fields": {"summary": "s", "project": {"key": "ENG"}}}],
            "isLast": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/users/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"accountId": "a1", "displayName": "Alice", "active": true}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/rest/api/3/issue/K-1/worklog"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "worklogs": [{"id": "1", "author": {"displayName": "Al"}, "timeSpentSeconds": 60, "started": "2026"}]
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn connector_real_mode_dispatches_every_table() {
    let server = full_server().await;
    let conn = JiraConnector::new(JiraConfig {
        base_url: server.uri(),
        email: "e@x.com".into(),
        api_token: "t".into(),
        mock: false,
    });

    let q = Query {
        limit: Some(5),
        ..Default::default()
    };
    assert!(!conn.fetch("projects", &q).await.unwrap().is_empty());
    assert!(!conn.fetch("issues", &q).await.unwrap().is_empty()); // exercises pushdown JQL
    assert!(!conn.fetch("users", &q).await.unwrap().is_empty());
    assert!(!conn.fetch("worklogs", &q).await.unwrap().is_empty());

    // Unknown table in real mode → UnknownTable.
    let err = conn.fetch("widgets", &q).await.unwrap_err();
    assert!(matches!(err, ConnectorError::UnknownTable(_)));
}
