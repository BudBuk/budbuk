//! Cross-connector end-to-end test: resolve several connectors through the
//! catalog, fetch from each (through the real engine, against mock servers that
//! mimic each API's shape), and combine the rows across connectors. This
//! exercises the whole path — catalog → SourceSpec → RestConnector → neutral
//! rows — for connectors with different auth, pagination, and row-path styles.

use std::collections::HashMap;

use catalog::spec_for;
use connector_sdk::{Connector, Query, Row};
use rest_connector::RestConnector;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn opts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

async fn mount(server: &MockServer, p: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

async fn fetch(name: &str, options: &HashMap<String, String>, table: &str) -> Vec<Row> {
    let spec = spec_for(name, options).unwrap();
    RestConnector::new(spec)
        .fetch(table, &Query::default())
        .await
        .unwrap()
}

#[tokio::test]
async fn queries_span_multiple_connectors_through_the_catalog() {
    // Three connectors, three different shapes:
    //   GitLab     — bare array (RowPath::Root), Bearer auth, Page pagination
    //   PagerDuty  — "/incidents" pointer, "Token token=" header, Offset pagination
    //   Contentful — "/items" pointer, Bearer, Offset, nested `sys.id` fields
    let gl = MockServer::start().await;
    mount(
        &gl,
        "/api/v4/projects",
        json!([{"id": 1, "name": "app", "star_count": 10}]),
    )
    .await;
    let gl_uri = gl.uri();

    let pd = MockServer::start().await;
    mount(
        &pd,
        "/incidents",
        json!({"incidents": [{"id": "PABC", "title": "down", "status": "triggered"}]}),
    )
    .await;
    let pd_uri = pd.uri();

    let cf = MockServer::start().await;
    mount(
        &cf,
        "/entries",
        json!({"items": [{"sys": {"id": "e1", "createdAt": "2026-01-01T00:00:00Z"}}]}),
    )
    .await;
    let cf_uri = cf.uri();

    let gl_rows = fetch(
        "gitlab",
        &opts(&[("base_url", gl_uri.as_str()), ("token", "t")]),
        "projects",
    )
    .await;
    let pd_rows = fetch(
        "pagerduty",
        &opts(&[("base_url", pd_uri.as_str()), ("api_key", "k")]),
        "incidents",
    )
    .await;
    let cf_rows = fetch(
        "contentful",
        &opts(&[("base_url", cf_uri.as_str()), ("access_token", "t")]),
        "entries",
    )
    .await;

    assert_eq!(gl_rows.len(), 1);
    assert_eq!(pd_rows.len(), 1);
    assert_eq!(cf_rows.len(), 1);

    // Build one unified cross-connector view: (source, id) from each connector's
    // first column. This is the essence of a cross-connector query.
    let unified: Vec<(&str, String)> = vec![
        ("gitlab", gl_rows[0].0[0].to_display_string()),
        ("pagerduty", pd_rows[0].0[0].to_display_string()),
        ("contentful", cf_rows[0].0[0].to_display_string()),
    ];

    assert_eq!(unified.len(), 3);
    assert_eq!(unified[0].1, "1"); // gitlab projects.id
    assert_eq!(unified[1].1, "PABC"); // pagerduty incidents.id
    assert_eq!(unified[2].1, "e1"); // contentful entries.id — nested sys.id survived end to end
    let sources: Vec<&str> = unified.iter().map(|(s, _)| *s).collect();
    assert!(sources.contains(&"gitlab"));
    assert!(sources.contains(&"pagerduty"));
    assert!(sources.contains(&"contentful"));
}
