//! BudBuk Jira connector demo CLI.
//!
//! This is a thin entrypoint; all logic lives in (and is tested via)
//! [`jira_connector::cli`]. It runs against a real Jira account when
//! JIRA_BASE_URL / JIRA_USER_EMAIL / JIRA_API_TOKEN are set (loaded from a local
//! `.env`), and falls back to mock mode otherwise.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    jira_connector::cli::run().await
}
