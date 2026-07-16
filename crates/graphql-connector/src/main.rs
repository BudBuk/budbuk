//! graphql-connector demo CLI. Thin entrypoint; all logic (and tests) live in
//! [`graphql_connector::cli`]. Generates a `GraphQlSpec` from a bundled
//! introspection document and runs it against the public Countries GraphQL API
//! — no credentials required — proving the generator feeds the same engine end
//! to end.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    graphql_connector::cli::run_introspect_at("https://countries.trevorblades.com/").await
}
