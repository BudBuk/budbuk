//! # github-connector
//!
//! The GitHub connector for BudBuk — and a demonstration that, once the engine
//! exists, a new connector is mostly *config*. It builds a
//! [`rest_connector::SourceSpec`] describing a few GitHub REST collections and
//! runs on the same `RestConnector` engine (caching, tracing, pushdown, FDW —
//! all for free).
//!
//! It works unauthenticated against public data (a low rate limit applies), or
//! with a personal access token for private/authenticated access.

pub mod cli;

use connector_sdk::DataType;
use rest_connector::{
    AuthSpec, ColumnSpec, FilterParam, Pagination, RowPath, SourceSpec, TableSpec,
};

/// One GitHub account/target. `owner`/`repo` select whose public collections to
/// expose; `token` (optional) authenticates. `base_url` supports GitHub
/// Enterprise and tests.
#[derive(Debug, Clone)]
pub struct GithubConfig {
    pub base_url: String,
    pub owner: String,
    pub repo: String,
    pub token: Option<String>,
}

impl GithubConfig {
    /// A config for public, unauthenticated access to `owner`/`repo`.
    pub fn public(owner: &str, repo: &str) -> Self {
        Self {
            base_url: "https://api.github.com".to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
            token: None,
        }
    }
}

/// Build the GitHub source spec for a config. This is the *entire* connector:
/// endpoints, columns, pagination, and pushdown, as data.
pub fn github_spec(cfg: &GithubConfig) -> SourceSpec {
    let col = |name: &str, field: &str, data_type: DataType| ColumnSpec {
        name: name.to_string(),
        field: field.to_string(),
        data_type,
    };
    // GitHub paginates with ?page=N&per_page=M (1-based).
    let page = || Pagination::Page {
        page_param: "page".to_string(),
        size_param: "per_page".to_string(),
        page_size: 30,
        start_page: 1,
    };
    let auth = match &cfg.token {
        Some(token) => AuthSpec::Bearer {
            token: token.clone(),
        },
        None => AuthSpec::None,
    };

    SourceSpec {
        name: "github".to_string(),
        base_url: cfg.base_url.clone(),
        auth,
        tables: vec![
            TableSpec {
                name: "repos".to_string(),
                path: format!("/users/{}/repos", cfg.owner),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("name", "name", DataType::Text),
                    col("full_name", "full_name", DataType::Text),
                    col("private", "private", DataType::Bool),
                    col("language", "language", DataType::Text),
                    col("stars", "stargazers_count", DataType::Integer),
                    col("forks", "forks_count", DataType::Integer),
                    col("owner", "owner.login", DataType::Text),
                    col("description", "description", DataType::Text),
                    col("created", "created_at", DataType::Timestamp),
                    col("updated", "updated_at", DataType::Timestamp),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "issues".to_string(),
                path: format!("/repos/{}/{}/issues", cfg.owner, cfg.repo),
                row_path: RowPath::Root,
                columns: vec![
                    col("number", "number", DataType::Integer),
                    col("title", "title", DataType::Text),
                    col("state", "state", DataType::Text),
                    col("user", "user.login", DataType::Text),
                    col("comments", "comments", DataType::Integer),
                    col("created", "created_at", DataType::Timestamp),
                    col("updated", "updated_at", DataType::Timestamp),
                ],
                pagination: page(),
                // `state` is a real column → WHERE state = 'closed' pushes down.
                filters: vec![FilterParam {
                    column: "state".to_string(),
                    param: "state".to_string(),
                }],
            },
            TableSpec {
                name: "gists".to_string(),
                path: format!("/users/{}/gists", cfg.owner),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Text),
                    col("description", "description", DataType::Text),
                    col("public", "public", DataType::Bool),
                    col("comments", "comments", DataType::Integer),
                    col("owner", "owner.login", DataType::Text),
                    col("created", "created_at", DataType::Timestamp),
                ],
                pagination: page(),
                filters: vec![],
            },
            TableSpec {
                name: "orgs".to_string(),
                path: format!("/users/{}/orgs", cfg.owner),
                row_path: RowPath::Root,
                columns: vec![
                    col("id", "id", DataType::Integer),
                    col("login", "login", DataType::Text),
                    col("description", "description", DataType::Text),
                    col("url", "url", DataType::Text),
                ],
                pagination: page(),
                filters: vec![],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_exposes_expected_tables_with_paths() {
        let cfg = GithubConfig::public("octocat", "Hello-World");
        let spec = github_spec(&cfg);
        assert_eq!(spec.name, "github");
        assert!(matches!(spec.auth, AuthSpec::None));
        assert_eq!(spec.table("repos").unwrap().path, "/users/octocat/repos");
        assert_eq!(
            spec.table("issues").unwrap().path,
            "/repos/octocat/Hello-World/issues"
        );
        // The issues table pushes down `state`.
        assert_eq!(spec.table("issues").unwrap().filters[0].param, "state");
        assert!(spec.table("gists").is_some());
        assert!(spec.table("orgs").is_some());
    }

    #[test]
    fn a_token_produces_bearer_auth() {
        let cfg = GithubConfig {
            token: Some("ghp_x".to_string()),
            ..GithubConfig::public("o", "r")
        };
        assert!(matches!(github_spec(&cfg).auth, AuthSpec::Bearer { .. }));
        // Exercise Debug/Clone on the config.
        let _ = cfg.clone();
        assert!(!format!("{cfg:?}").is_empty());
    }
}
