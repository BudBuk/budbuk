//! The real HTTP client for Jira Cloud's REST API.
//!
//! Concepts here:
//!
//! - `reqwest` — an async HTTP client (build request, `.await` response).
//! - `serde` deserialization — describe the JSON shape as Rust structs marked
//!   `#[derive(Deserialize)]`; serde fills them in.
//! - Pagination — APIs return data in pages. Jira uses two styles: token-based
//!   (`nextPageToken`) for issue search, and offset-based (`startAt`) for
//!   projects and users. We loop until we have enough rows or run out of pages.

use std::time::Instant;

use connector_sdk::{ConnectorError, Result, Row, Value};
use serde::de::DeserializeOwned;
use serde::Deserialize;

/// Jira's largest allowed page size for issue search.
const ISSUE_PAGE: usize = 100;
/// A comfortable page size for project/user search.
const OFFSET_PAGE: usize = 50;

/// A thin async HTTP client bound to ONE Jira account's URL + credentials.
pub struct JiraClient {
    http: reqwest::Client,
    base_url: String,
    email: String,
    api_token: String,
}

impl JiraClient {
    pub fn new(base_url: String, email: String, api_token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url,
            email,
            api_token,
        }
    }

    /// Shared helper: GET `url` with basic auth + query params, parse JSON into
    /// any `T`. Every endpoint below is built on this one function.
    async fn get_json<T: DeserializeOwned>(&self, url: &str, query: &[(&str, &str)]) -> Result<T> {
        let started = Instant::now();
        let response = self
            .http
            .get(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .header("Accept", "application/json")
            .query(query)
            .send()
            .await
            .map_err(|e| ConnectorError::Network(e.to_string()))?;

        let status = response.status();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        tracing::debug!(target: "budbuk::jira", %url, status = status.as_u16(), elapsed_ms);
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(match status.as_u16() {
                401 | 403 => ConnectorError::Auth(format!("{status}: {body}")),
                _ => ConnectorError::Other(format!("HTTP {status}: {body}")),
            });
        }

        response
            .json::<T>()
            .await
            .map_err(|e| ConnectorError::Parse(e.to_string()))
    }

    /// Projects — offset-based pagination via `startAt`, following `isLast`.
    pub async fn projects(&self, limit: usize) -> Result<Vec<Row>> {
        let url = format!("{}/rest/api/3/project/search", self.base_url);
        let mut rows: Vec<Row> = Vec::new();
        let mut start_at = 0usize;

        loop {
            // Ask for only as many as we still need, capped at the page size.
            let want = (limit - rows.len()).min(OFFSET_PAGE);
            let (want_s, start_s) = (want.to_string(), start_at.to_string());
            let resp: ProjectSearchResponse = self
                .get_json(
                    &url,
                    &[
                        ("maxResults", &want_s),
                        ("startAt", &start_s),
                        ("expand", "lead"),
                    ],
                )
                .await?;

            let got = resp.values.len();
            rows.extend(resp.values.into_iter().map(project_to_row));
            start_at += got;

            // Stop when full, told it's the last page, or the page was empty.
            if rows.len() >= limit || resp.is_last.unwrap_or(true) || got == 0 {
                break;
            }
        }
        rows.truncate(limit);
        Ok(rows)
    }

    /// Users — offset-based pagination. This endpoint returns a bare array with
    /// no `isLast`, so we stop when a page comes back smaller than requested.
    pub async fn users(&self, limit: usize) -> Result<Vec<Row>> {
        let url = format!("{}/rest/api/3/users/search", self.base_url);
        let mut rows: Vec<Row> = Vec::new();
        let mut start_at = 0usize;

        loop {
            let want = (limit - rows.len()).min(OFFSET_PAGE);
            let (want_s, start_s) = (want.to_string(), start_at.to_string());
            let page: Vec<JiraUser> = self
                .get_json(&url, &[("maxResults", &want_s), ("startAt", &start_s)])
                .await?;

            let got = page.len();
            rows.extend(page.into_iter().map(user_to_row));
            start_at += got;

            // A short page means we've reached the end.
            if rows.len() >= limit || got < want || got == 0 {
                break;
            }
        }
        rows.truncate(limit);
        Ok(rows)
    }

    /// Issues — token-based pagination. Each response carries a `nextPageToken`
    /// we feed into the next request until `isLast` is true.
    pub async fn issues(&self, jql: &str, limit: usize) -> Result<Vec<Row>> {
        let url = format!("{}/rest/api/3/search/jql", self.base_url);
        let mut rows: Vec<Row> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let want = (limit - rows.len()).min(ISSUE_PAGE).to_string();
            let mut params: Vec<(&str, &str)> = vec![
                ("jql", jql),
                ("fields", "summary,status,assignee,project,created"),
                ("maxResults", &want),
            ];
            // Include the token only from the second page onward.
            if let Some(token) = page_token.as_deref() {
                params.push(("nextPageToken", token));
            }

            let resp: IssueSearchResponse = self.get_json(&url, &params).await?;
            rows.extend(resp.issues.into_iter().map(issue_to_row));

            // Continue only if there's a token AND we still want more.
            match resp.next_page_token {
                Some(token) if rows.len() < limit && resp.is_last != Some(true) => {
                    page_token = Some(token);
                }
                _ => break,
            }
        }
        rows.truncate(limit);
        Ok(rows)
    }

    /// Worklogs — there's no global worklog list, so we first find recent
    /// issues that HAVE worklogs, then fetch each issue's worklogs and flatten
    /// them. (This is a classic "N+1" fetch; we parallelize it in a later step.)
    pub async fn worklogs(&self, jql: &str, limit: usize) -> Result<Vec<Row>> {
        let keys = self.issue_keys(jql, limit).await?;

        let mut rows: Vec<Row> = Vec::new();
        for key in keys {
            if rows.len() >= limit {
                break;
            }
            let url = format!("{}/rest/api/3/issue/{}/worklog", self.base_url, key);
            let resp: WorklogResponse = self.get_json(&url, &[("maxResults", "50")]).await?;
            for w in resp.worklogs {
                rows.push(worklog_to_row(&key, w));
                if rows.len() >= limit {
                    break;
                }
            }
        }
        Ok(rows)
    }

    /// Helper for worklogs: get up to `limit` recent issue keys for a JQL.
    async fn issue_keys(&self, jql: &str, limit: usize) -> Result<Vec<String>> {
        let url = format!("{}/rest/api/3/search/jql", self.base_url);
        let want = limit.min(ISSUE_PAGE).to_string();
        let resp: IssueSearchResponse = self
            .get_json(
                &url,
                &[("jql", jql), ("fields", "summary"), ("maxResults", &want)],
            )
            .await?;
        Ok(resp.issues.into_iter().map(|i| i.key).collect())
    }
}

// --- Row converters: turn one API record into a neutral Row. Keeping these as
// --- free functions makes the paginating loops above read cleanly.

fn project_to_row(p: JiraProject) -> Row {
    Row(vec![
        Value::Integer(p.id.parse().unwrap_or(0)),
        Value::Text(p.key),
        Value::Text(p.name),
        opt_text(p.lead.and_then(|l| l.display_name)),
    ])
}

fn issue_to_row(i: IssueItem) -> Row {
    let f = i.fields;
    Row(vec![
        Value::Text(i.key),
        opt_text(f.summary),
        opt_text(f.status.and_then(|s| s.name)),
        opt_text(f.assignee.and_then(|a| a.display_name)),
        opt_text(f.project.and_then(|p| p.key)),
        opt_ts(f.created),
    ])
}

fn user_to_row(u: JiraUser) -> Row {
    Row(vec![
        Value::Text(u.account_id),
        opt_text(u.display_name),
        opt_text(u.email_address),
        Value::Bool(u.active.unwrap_or(false)),
    ])
}

fn worklog_to_row(issue_key: &str, w: WorklogItem) -> Row {
    Row(vec![
        Value::Integer(w.id.parse().unwrap_or(0)),
        Value::Text(issue_key.to_string()),
        opt_text(w.author.and_then(|a| a.display_name)),
        match w.time_spent_seconds {
            Some(n) => Value::Integer(n),
            None => Value::Null,
        },
        opt_ts(w.started),
    ])
}

fn opt_text(s: Option<String>) -> Value {
    match s {
        Some(v) => Value::Text(v),
        None => Value::Null,
    }
}

fn opt_ts(s: Option<String>) -> Value {
    match s {
        Some(v) => Value::Timestamp(v),
        None => Value::Null,
    }
}

// --- Expected JSON shapes. Unlisted fields are ignored; Option tolerates nulls.

#[derive(Deserialize)]
struct ProjectSearchResponse {
    values: Vec<JiraProject>,
    #[serde(rename = "isLast")]
    is_last: Option<bool>,
}

#[derive(Deserialize)]
struct JiraProject {
    id: String,
    key: String,
    name: String,
    #[serde(default)]
    lead: Option<UserRef>,
}

/// A user reference embedded in projects (lead), issues (assignee), worklogs.
#[derive(Deserialize)]
struct UserRef {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct IssueSearchResponse {
    issues: Vec<IssueItem>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
    #[serde(rename = "isLast")]
    is_last: Option<bool>,
}

#[derive(Deserialize)]
struct IssueItem {
    key: String,
    fields: IssueFields,
}

#[derive(Deserialize)]
struct IssueFields {
    summary: Option<String>,
    status: Option<Named>,
    assignee: Option<UserRef>,
    project: Option<ProjectKeyRef>,
    created: Option<String>,
}

#[derive(Deserialize)]
struct Named {
    name: Option<String>,
}

#[derive(Deserialize)]
struct ProjectKeyRef {
    key: Option<String>,
}

#[derive(Deserialize)]
struct JiraUser {
    #[serde(rename = "accountId")]
    account_id: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
    active: Option<bool>,
}

#[derive(Deserialize)]
struct WorklogResponse {
    worklogs: Vec<WorklogItem>,
}

#[derive(Deserialize)]
struct WorklogItem {
    id: String,
    author: Option<UserRef>,
    #[serde(rename = "timeSpentSeconds")]
    time_spent_seconds: Option<i64>,
    started: Option<String>,
}
