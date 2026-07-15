//! HTTP integration tests for the RestConnector engine, using `wiremock` as a
//! fake API. Covers pagination styles, predicate pushdown, auth variants, row
//! extraction, and every error path — no real network.

use connector_sdk::{Connector, ConnectorError, DataType, Filter, Operator, Query, Value};
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RestConnector, RowPath, SourceSpec, TableSpec,
};
use serde_json::json;
use wiremock::matchers::{header, header_exists, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn col(name: &str, field: &str, data_type: DataType) -> ColumnSpec {
    ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    }
}

/// A one-table source with configurable path/columns/pagination/filters/auth.
fn source(base_url: &str, table: TableSpec, auth: AuthSpec) -> SourceSpec {
    SourceSpec {
        name: "test".into(),
        base_url: base_url.to_string(),
        auth,
        tables: vec![table],
    }
}

fn simple_table(pagination: Pagination) -> TableSpec {
    TableSpec {
        name: "items".into(),
        path: "/items".into(),
        row_path: RowPath::Root,
        columns: vec![
            col("id", "id", DataType::Integer),
            col("name", "name", DataType::Text),
            col("k", "meta.k", DataType::Text),
        ],
        pagination,
        filters: vec![],
    }
}

#[tokio::test]
async fn discover_lists_tables_and_columns() {
    let spec = source(
        "http://unused",
        simple_table(Pagination::None),
        AuthSpec::None,
    );
    let schemas = RestConnector::new(spec).discover().await.unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].name, "items");
    assert_eq!(schemas[0].columns.len(), 3);
}

#[tokio::test]
async fn fetch_none_pagination_maps_rows_including_nested_and_null() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "name": "a", "meta": {"k": "v"}},
            {"id": 2}
        ])))
        .mount(&server)
        .await;

    let spec = source(
        &server.uri(),
        simple_table(Pagination::None),
        AuthSpec::None,
    );
    let rows = RestConnector::new(spec)
        .fetch("items", &Query::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0[2].to_display_string(), "v"); // nested meta.k
    assert_eq!(rows[1].0[1].to_display_string(), "NULL"); // missing name
    assert_eq!(rows[1].0[2].to_display_string(), "NULL"); // missing meta.k
}

#[tokio::test]
async fn fetch_offset_pagination_walks_pages() {
    let server = MockServer::start().await;
    for (start, ids) in [("0", vec![1, 2]), ("2", vec![3, 4]), ("4", vec![5])] {
        Mock::given(method("GET"))
            .and(path("/items"))
            .and(query_param("_start", start))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(ids
                .iter()
                .map(|i| json!({"id": i, "name": "x"}))
                .collect::<Vec<_>>())))
            .mount(&server)
            .await;
    }
    let table = simple_table(Pagination::Offset {
        start_param: "_start".into(),
        limit_param: "_limit".into(),
        page_size: 2,
    });
    let spec = source(&server.uri(), table, AuthSpec::None);
    // limit 10 > available (5): the loop stops on the short last page.
    let q = Query {
        limit: Some(10),
        ..Default::default()
    };
    let rows = RestConnector::new(spec).fetch("items", &q).await.unwrap();
    assert_eq!(rows.len(), 5);
}

#[tokio::test]
async fn fetch_page_pagination_walks_pages() {
    let server = MockServer::start().await;
    for (page, ids) in [("1", vec![1, 2]), ("2", vec![3, 4]), ("3", vec![5])] {
        Mock::given(method("GET"))
            .and(path("/items"))
            .and(query_param("page", page))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(ids
                .iter()
                .map(|i| json!({"id": i, "name": "x"}))
                .collect::<Vec<_>>())))
            .mount(&server)
            .await;
    }
    let table = simple_table(Pagination::Page {
        page_param: "page".into(),
        size_param: "per_page".into(),
        page_size: 2,
        start_page: 1,
    });
    let spec = source(&server.uri(), table, AuthSpec::None);
    let q = Query {
        limit: Some(10),
        ..Default::default()
    };
    let rows = RestConnector::new(spec).fetch("items", &q).await.unwrap();
    assert_eq!(rows.len(), 5);
}

#[tokio::test]
async fn fetch_cursor_pagination_follows_has_more() {
    let server = MockServer::start().await;
    // Stripe-style: {data:[...], has_more:bool}. Distinguish pages by `limit`
    // value (page 1 requests limit=2, page 2 requests limit=1).
    Mock::given(method("GET"))
        .and(path("/items"))
        .and(query_param("limit", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": "a"}, {"id": "b"}], "has_more": true
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .and(query_param("starting_after", "b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": "c"}], "has_more": false
        })))
        .mount(&server)
        .await;

    let table = TableSpec {
        name: "items".into(),
        path: "/items".into(),
        row_path: RowPath::Pointer {
            pointer: "/data".into(),
        },
        columns: vec![col("id", "id", DataType::Text)],
        pagination: Pagination::Cursor {
            limit_param: "limit".into(),
            cursor_param: "starting_after".into(),
            cursor_field: "id".into(),
            more_pointer: "/has_more".into(),
            page_size: 2,
        },
        filters: vec![],
    };
    let spec = source(&server.uri(), table, AuthSpec::None);
    let q = Query {
        limit: Some(3),
        ..Default::default()
    };
    let rows = RestConnector::new(spec).fetch("items", &q).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2].0[0].to_display_string(), "c");
}

#[tokio::test]
async fn fetch_reads_rows_from_a_pointer() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": 1, "name": "a"}, {"id": 2, "name": "b"}]
        })))
        .mount(&server)
        .await;
    let mut table = simple_table(Pagination::None);
    table.row_path = RowPath::Pointer {
        pointer: "/data".into(),
    };
    let spec = source(&server.uri(), table, AuthSpec::None);
    let rows = RestConnector::new(spec)
        .fetch("items", &Query::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn fetch_pushes_down_equality_filter_as_param() {
    let server = MockServer::start().await;
    // Only matches if the pushed-down param is present.
    Mock::given(method("GET"))
        .and(path("/items"))
        .and(query_param("owner", "alice"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1, "name": "a"}])))
        .mount(&server)
        .await;
    let mut table = simple_table(Pagination::None);
    table.filters = vec![FilterParam {
        column: "name".into(),
        param: "owner".into(),
    }];
    let spec = source(&server.uri(), table, AuthSpec::None);
    let q = Query {
        filters: vec![Filter::new(
            "name",
            Operator::Eq,
            Value::Text("alice".into()),
        )],
        ..Default::default()
    };
    let rows = RestConnector::new(spec).fetch("items", &q).await.unwrap();
    assert_eq!(rows.len(), 1);
}

async fn assert_auth_reaches_server(auth: AuthSpec, mock: Mock) {
    let server = MockServer::start().await;
    mock.mount(&server).await;
    let spec = source(&server.uri(), simple_table(Pagination::None), auth);
    let rows = RestConnector::new(spec)
        .fetch("items", &Query::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn bearer_auth_is_applied() {
    let m = Mock::given(method("GET"))
        .and(path("/items"))
        .and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])));
    assert_auth_reaches_server(
        AuthSpec::Bearer {
            token: "tok".into(),
        },
        m,
    )
    .await;
}

#[tokio::test]
async fn basic_auth_is_applied() {
    let m = Mock::given(method("GET"))
        .and(path("/items"))
        .and(header_exists("authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])));
    assert_auth_reaches_server(
        AuthSpec::Basic {
            username: "u".into(),
            password: "p".into(),
        },
        m,
    )
    .await;
}

#[tokio::test]
async fn api_key_header_is_applied() {
    let m = Mock::given(method("GET"))
        .and(path("/items"))
        .and(header("x-api-key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])));
    assert_auth_reaches_server(
        AuthSpec::ApiKeyHeader {
            header: "X-API-Key".into(),
            value: "secret".into(),
        },
        m,
    )
    .await;
}

#[tokio::test]
async fn api_key_query_is_applied() {
    let m = Mock::given(method("GET"))
        .and(path("/items"))
        .and(query_param("api_key", "secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])));
    assert_auth_reaches_server(
        AuthSpec::ApiKeyQuery {
            param: "api_key".into(),
            value: "secret".into(),
        },
        m,
    )
    .await;
}

#[tokio::test]
async fn requests_carry_a_default_user_agent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .and(header_exists("user-agent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 1}])))
        .mount(&server)
        .await;
    let spec = source(
        &server.uri(),
        simple_table(Pagination::None),
        AuthSpec::None,
    );
    let rows = RestConnector::new(spec)
        .fetch("items", &Query::default())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn unknown_table_errors() {
    let spec = source(
        "http://unused",
        simple_table(Pagination::None),
        AuthSpec::None,
    );
    let err = RestConnector::new(spec)
        .fetch("nope", &Query::default())
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::UnknownTable(_)));
}

async fn error_case(status: u16, body: &str) -> ConnectorError {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/items"))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(&server)
        .await;
    let spec = source(
        &server.uri(),
        simple_table(Pagination::None),
        AuthSpec::None,
    );
    RestConnector::new(spec)
        .fetch("items", &Query::default())
        .await
        .unwrap_err()
}

#[tokio::test]
async fn auth_error_maps_to_auth() {
    assert!(matches!(
        error_case(401, "no").await,
        ConnectorError::Auth(_)
    ));
}

#[tokio::test]
async fn server_error_maps_to_other() {
    assert!(matches!(
        error_case(500, "boom").await,
        ConnectorError::Other(_)
    ));
}

#[tokio::test]
async fn invalid_json_maps_to_parse() {
    assert!(matches!(
        error_case(200, "not json").await,
        ConnectorError::Parse(_)
    ));
}

#[tokio::test]
async fn non_array_body_maps_to_parse() {
    // 200 OK but the body isn't an array where we expect rows.
    assert!(matches!(
        error_case(200, "{}").await,
        ConnectorError::Parse(_)
    ));
}

#[tokio::test]
async fn unreachable_host_maps_to_network() {
    let spec = source(
        "http://127.0.0.1:1",
        simple_table(Pagination::None),
        AuthSpec::None,
    );
    let err = RestConnector::new(spec)
        .fetch("items", &Query::default())
        .await
        .unwrap_err();
    assert!(matches!(err, ConnectorError::Network(_)));
}
