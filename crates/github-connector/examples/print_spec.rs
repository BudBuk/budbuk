//! Print the GitHub SourceSpec as JSON, for embedding in the rest-fdw server
//! options. Run: cargo run -p github-connector --example print_spec

fn main() {
    let owner = std::env::var("GITHUB_OWNER").unwrap_or_else(|_| "octocat".to_string());
    let repo = std::env::var("GITHUB_REPO").unwrap_or_else(|_| "Hello-World".to_string());
    let mut cfg = github_connector::GithubConfig::public(&owner, &repo);
    cfg.token = std::env::var("GITHUB_TOKEN").ok();
    let spec = github_connector::github_spec(&cfg);
    println!("{}", serde_json::to_string(&spec).unwrap());
}
