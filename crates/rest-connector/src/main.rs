//! rest-connector demo CLI. Thin entrypoint; all logic (and tests) live in
//! [`rest_connector::cli`]. Generates a `SourceSpec` from a bundled OpenAPI
//! document and runs it against the public JSONPlaceholder API — no credentials
//! required — proving the OpenAPI importer feeds the same engine end to end.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    rest_connector::cli::run_openapi_at("https://jsonplaceholder.typicode.com").await
}
