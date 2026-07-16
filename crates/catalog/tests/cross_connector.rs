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

#[tokio::test]
async fn batch1_connectors_span_the_catalog() {
    // Three more connectors, three more shapes:
    //   asana   — "/data" pointer, Bearer, no pagination
    //   shopify — "/products" pointer, X-Shopify-Access-Token header
    //   sentry  — bare array (RowPath::Root), Bearer
    let asana = MockServer::start().await;
    mount(
        &asana,
        "/projects",
        json!({"data": [{"gid": "111", "name": "Launch", "archived": false}]}),
    )
    .await;

    let shopify = MockServer::start().await;
    mount(
        &shopify,
        "/products.json",
        json!({"products": [{"id": 4242, "title": "Tee", "status": "active", "vendor": "Acme"}]}),
    )
    .await;

    let sentry = MockServer::start().await;
    mount(
        &sentry,
        "/projects/",
        json!([{"id": "p9", "slug": "web", "name": "Web", "platform": "javascript"}]),
    )
    .await;

    let asana_rows = fetch(
        "asana",
        &opts(&[("base_url", asana.uri().as_str()), ("token", "t")]),
        "projects",
    )
    .await;
    let shopify_rows = fetch(
        "shopify",
        &opts(&[("base_url", shopify.uri().as_str()), ("access_token", "t")]),
        "products",
    )
    .await;
    let sentry_rows = fetch(
        "sentry",
        &opts(&[("base_url", sentry.uri().as_str()), ("token", "t")]),
        "projects",
    )
    .await;

    let unified: Vec<(&str, String)> = vec![
        ("asana", asana_rows[0].0[0].to_display_string()),
        ("shopify", shopify_rows[0].0[0].to_display_string()),
        ("sentry", sentry_rows[0].0[0].to_display_string()),
    ];
    assert_eq!(unified[0].1, "111"); // asana projects.gid
    assert_eq!(unified[1].1, "4242"); // shopify products.id
    assert_eq!(unified[2].1, "p9"); // sentry projects.id
}

#[tokio::test]
async fn batch2_connectors_span_the_catalog() {
    //   hubspot    — "/results" pointer, Bearer
    //   slack      — "/members" pointer, Bearer
    //   servicenow — "/result" pointer, Basic, Offset pagination
    let hs = MockServer::start().await;
    mount(
        &hs,
        "/crm/v3/objects/contacts",
        json!({"results": [{"id": "c1", "createdAt": "2026-01-01T00:00:00Z",
                            "updatedAt": "2026-01-02T00:00:00Z", "archived": false}]}),
    )
    .await;

    let sl = MockServer::start().await;
    mount(
        &sl,
        "/users.list",
        json!({"members": [{"id": "U1", "name": "bob", "real_name": "Bob"}]}),
    )
    .await;

    let sn = MockServer::start().await;
    mount(
        &sn,
        "/table/incident",
        json!({"result": [{"sys_id": "s1", "number": "INC001",
                           "short_description": "down", "state": "1"}]}),
    )
    .await;

    let hs_rows = fetch(
        "hubspot",
        &opts(&[("base_url", hs.uri().as_str()), ("token", "t")]),
        "contacts",
    )
    .await;
    let sl_rows = fetch(
        "slack",
        &opts(&[("base_url", sl.uri().as_str()), ("token", "t")]),
        "users",
    )
    .await;
    let sn_rows = fetch(
        "servicenow",
        &opts(&[
            ("base_url", sn.uri().as_str()),
            ("username", "u"),
            ("password", "p"),
        ]),
        "incident",
    )
    .await;

    assert_eq!(hs_rows[0].0[0].to_display_string(), "c1");
    assert_eq!(sl_rows[0].0[0].to_display_string(), "U1");
    assert_eq!(sn_rows[0].0[0].to_display_string(), "s1");
}

#[tokio::test]
async fn batch3_connectors_span_the_catalog() {
    //   okta     — root array, SSWS header
    //   twilio   — "/messages" pointer, Basic, Page pagination
    //   opsgenie — "/data" pointer, GenieKey header, Offset pagination
    let ok = MockServer::start().await;
    mount(
        &ok,
        "/users",
        json!([{"id": "u1", "status": "ACTIVE", "created": "2026-01-01T00:00:00Z"}]),
    )
    .await;

    let tw = MockServer::start().await;
    mount(&tw, "/Messages.json", json!({"messages": [{"sid": "SM1", "status": "sent", "to": "+1", "from": "+2", "body": "hi"}]})).await;

    let og = MockServer::start().await;
    mount(
        &og,
        "/alerts",
        json!({"data": [{"id": "a1", "message": "down", "status": "open", "priority": "P1"}]}),
    )
    .await;

    let ok_rows = fetch(
        "okta",
        &opts(&[("base_url", ok.uri().as_str()), ("token", "t")]),
        "users",
    )
    .await;
    let tw_rows = fetch(
        "twilio",
        &opts(&[
            ("base_url", tw.uri().as_str()),
            ("account_sid", "AC"),
            ("auth_token", "t"),
        ]),
        "messages",
    )
    .await;
    let og_rows = fetch(
        "opsgenie",
        &opts(&[("base_url", og.uri().as_str()), ("api_key", "k")]),
        "alerts",
    )
    .await;

    assert_eq!(ok_rows[0].0[0].to_display_string(), "u1");
    assert_eq!(tw_rows[0].0[0].to_display_string(), "SM1");
    assert_eq!(og_rows[0].0[0].to_display_string(), "a1");
}

#[tokio::test]
async fn batch4_connectors_span_the_catalog() {
    //   smartsheet — "/data" pointer, Bearer, Page
    //   bitbucket  — "/values" pointer, Basic, Page
    //   recurly    — "/data" pointer, Basic (api_key as username)
    let sm = MockServer::start().await;
    mount(
        &sm,
        "/sheets",
        json!({"data": [{"id": 1, "name": "Q1 Plan"}]}),
    )
    .await;
    let bb = MockServer::start().await;
    mount(
        &bb,
        "/repositories",
        json!({"values": [{"uuid": "{r1}", "name": "app", "full_name": "acme/app"}]}),
    )
    .await;
    let rc = MockServer::start().await;
    mount(
        &rc,
        "/accounts",
        json!({"data": [{"id": "a1", "code": "acme", "state": "active"}]}),
    )
    .await;

    let sm_rows = fetch(
        "smartsheet",
        &opts(&[("base_url", sm.uri().as_str()), ("token", "t")]),
        "sheets",
    )
    .await;
    let bb_rows = fetch(
        "bitbucket",
        &opts(&[
            ("base_url", bb.uri().as_str()),
            ("username", "u"),
            ("app_password", "p"),
        ]),
        "repositories",
    )
    .await;
    let rc_rows = fetch(
        "recurly",
        &opts(&[("base_url", rc.uri().as_str()), ("api_key", "k")]),
        "accounts",
    )
    .await;

    assert_eq!(sm_rows[0].0[0].to_display_string(), "1");
    assert_eq!(bb_rows[0].0[0].to_display_string(), "{r1}");
    assert_eq!(rc_rows[0].0[0].to_display_string(), "a1");
}

#[tokio::test]
async fn batch5_connectors_span_the_catalog() {
    //   confluence  — "/results" pointer, Basic, Offset
    //   woocommerce — root array, Basic, Page
    //   zohocrm     — "/data" pointer, Zoho-oauthtoken header, Page
    let cf = MockServer::start().await;
    mount(
        &cf,
        "/content",
        json!({"results": [{"id": "c1", "type": "page", "title": "Home", "status": "current"}]}),
    )
    .await;
    let wc = MockServer::start().await;
    mount(
        &wc,
        "/products",
        json!([{"id": 7, "name": "Hat", "status": "publish", "price": "9.99"}]),
    )
    .await;
    let zc = MockServer::start().await;
    mount(
        &zc,
        "/Leads",
        json!({"data": [{"id": "L1", "Email": "a@b.c", "Company": "Acme"}]}),
    )
    .await;

    let cf_rows = fetch(
        "confluence",
        &opts(&[
            ("base_url", cf.uri().as_str()),
            ("email", "a@b.c"),
            ("api_token", "t"),
        ]),
        "content",
    )
    .await;
    let wc_rows = fetch(
        "woocommerce",
        &opts(&[
            ("base_url", wc.uri().as_str()),
            ("consumer_key", "ck"),
            ("consumer_secret", "cs"),
        ]),
        "products",
    )
    .await;
    let zc_rows = fetch(
        "zohocrm",
        &opts(&[("base_url", zc.uri().as_str()), ("token", "t")]),
        "Leads",
    )
    .await;

    assert_eq!(cf_rows[0].0[0].to_display_string(), "c1");
    assert_eq!(wc_rows[0].0[0].to_display_string(), "7");
    assert_eq!(zc_rows[0].0[0].to_display_string(), "L1");
}

#[tokio::test]
async fn batch6_connectors_span_the_catalog() {
    //   sendgrid   — root array (bounces), Bearer
    //   greenhouse — root array, Basic (api_key as username), Page
    //   chargebee  — "/list" pointer with nested subscription.*, Basic
    let sg = MockServer::start().await;
    mount(
        &sg,
        "/suppression/bounces",
        json!([{"email": "a@b.c", "reason": "550", "created": 123}]),
    )
    .await;
    let gh = MockServer::start().await;
    mount(
        &gh,
        "/candidates",
        json!([{"id": 9, "first_name": "Ada", "last_name": "L"}]),
    )
    .await;
    let cb = MockServer::start().await;
    mount(
        &cb,
        "/subscriptions",
        json!({"list": [{"subscription": {"id": "sub_1", "status": "active"}}]}),
    )
    .await;

    let sg_rows = fetch(
        "sendgrid",
        &opts(&[("base_url", sg.uri().as_str()), ("api_key", "k")]),
        "bounces",
    )
    .await;
    let gh_rows = fetch(
        "greenhouse",
        &opts(&[("base_url", gh.uri().as_str()), ("api_key", "k")]),
        "candidates",
    )
    .await;
    let cb_rows = fetch(
        "chargebee",
        &opts(&[("base_url", cb.uri().as_str()), ("api_key", "k")]),
        "subscriptions",
    )
    .await;

    assert_eq!(sg_rows[0].0[0].to_display_string(), "a@b.c");
    assert_eq!(gh_rows[0].0[0].to_display_string(), "9");
    assert_eq!(cb_rows[0].0[0].to_display_string(), "sub_1"); // nested subscription.id survived
}

#[tokio::test]
async fn batch7_connectors_span_the_catalog() {
    //   paypal  — "/plans" pointer, Bearer
    //   box     — "/entries" pointer, Bearer
    //   grafana — root array, Bearer
    let pp = MockServer::start().await;
    mount(
        &pp,
        "/v1/billing/plans",
        json!({"plans": [{"id": "P1", "name": "Gold", "status": "ACTIVE"}]}),
    )
    .await;
    let bx = MockServer::start().await;
    mount(
        &bx,
        "/users",
        json!({"entries": [{"id": "u1", "name": "Ada", "login": "a@b.c"}]}),
    )
    .await;
    let gf = MockServer::start().await;
    mount(
        &gf,
        "/datasources",
        json!([{"id": 3, "name": "Prom", "type": "prometheus"}]),
    )
    .await;

    let pp_rows = fetch(
        "paypal",
        &opts(&[("base_url", pp.uri().as_str()), ("token", "t")]),
        "plans",
    )
    .await;
    let bx_rows = fetch(
        "box",
        &opts(&[("base_url", bx.uri().as_str()), ("token", "t")]),
        "users",
    )
    .await;
    let gf_rows = fetch(
        "grafana",
        &opts(&[("base_url", gf.uri().as_str()), ("token", "t")]),
        "datasources",
    )
    .await;

    assert_eq!(pp_rows[0].0[0].to_display_string(), "P1");
    assert_eq!(bx_rows[0].0[0].to_display_string(), "u1");
    assert_eq!(gf_rows[0].0[0].to_display_string(), "3");
}

#[tokio::test]
async fn batch8_connectors_span_the_catalog() {
    //   klaviyo — "/data" pointer, nested attributes.*, multi-header auth
    //   datadog — root array, multi-header auth
    //   msgraph — "/value" pointer, Bearer
    let kv = MockServer::start().await;
    mount(
        &kv,
        "/profiles",
        json!({"data": [{"id": "pr1", "attributes": {"email": "jane@x.com"}}]}),
    )
    .await;
    let dd = MockServer::start().await;
    mount(
        &dd,
        "/v1/monitor",
        json!([{"id": 55, "name": "CPU", "type": "metric alert"}]),
    )
    .await;
    let mg = MockServer::start().await;
    mount(
        &mg,
        "/users",
        json!({"value": [{"id": "mu1", "displayName": "Ada", "mail": "a@b.c"}]}),
    )
    .await;

    let kv_rows = fetch(
        "klaviyo",
        &opts(&[("base_url", kv.uri().as_str()), ("api_key", "k")]),
        "profiles",
    )
    .await;
    let dd_rows = fetch(
        "datadog",
        &opts(&[
            ("base_url", dd.uri().as_str()),
            ("api_key", "k"),
            ("app_key", "a"),
        ]),
        "monitors",
    )
    .await;
    let mg_rows = fetch(
        "msgraph",
        &opts(&[("base_url", mg.uri().as_str()), ("token", "t")]),
        "users",
    )
    .await;

    assert_eq!(kv_rows[0].0[1].to_display_string(), "jane@x.com"); // nested attributes.email
    assert_eq!(dd_rows[0].0[0].to_display_string(), "55");
    assert_eq!(mg_rows[0].0[0].to_display_string(), "mu1");
}

#[tokio::test]
async fn batch9_connectors_span_the_catalog() {
    //   gcalendar — "/items" pointer, Bearer
    //   notion    — "/results" pointer, multi-header (Bearer + Notion-Version)
    let gc = MockServer::start().await;
    mount(
        &gc,
        "/users/me/calendarList",
        json!({"items": [{"id": "cal1", "summary": "Work", "timeZone": "UTC"}]}),
    )
    .await;
    let nt = MockServer::start().await;
    mount(
        &nt,
        "/users",
        json!({"results": [{"id": "nu1", "name": "Ada", "type": "person"}]}),
    )
    .await;

    let gc_rows = fetch(
        "gcalendar",
        &opts(&[("base_url", gc.uri().as_str()), ("token", "t")]),
        "calendars",
    )
    .await;
    let nt_rows = fetch(
        "notion",
        &opts(&[("base_url", nt.uri().as_str()), ("token", "t")]),
        "users",
    )
    .await;

    assert_eq!(gc_rows[0].0[0].to_display_string(), "cal1");
    assert_eq!(nt_rows[0].0[0].to_display_string(), "nu1");
}
