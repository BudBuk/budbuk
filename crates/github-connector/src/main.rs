//! GitHub connector demo CLI. Thin entrypoint; logic and tests live in
//! [`github_connector::cli`]. Runs against public GitHub data for
//! `octocat/Hello-World` by default (no token needed). Override with the
//! `GITHUB_OWNER`, `GITHUB_REPO`, and `GITHUB_TOKEN` environment variables.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let cfg = github_connector::cli::build_config_from_env();
    github_connector::cli::run_at(&cfg).await
}
