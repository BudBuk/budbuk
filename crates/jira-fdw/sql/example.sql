-- Example setup for the BudBuk Jira Foreign Data Wrapper.
--
-- Replace the placeholder credentials below. For this proof of concept the
-- credentials live in the SERVER options; a hardened deployment should source
-- secrets from a secrets manager instead.

-- 1. Load the extension and register the foreign data wrapper.
CREATE EXTENSION IF NOT EXISTS jira_fdw;

CREATE FOREIGN DATA WRAPPER jira_wrapper
    HANDLER jira_fdw_handler
    VALIDATOR jira_fdw_validator;

-- 2. Define an account (one foreign server per Jira instance).
CREATE SERVER jira_account
    FOREIGN DATA WRAPPER jira_wrapper
    OPTIONS (
        base_url  'https://your-domain.atlassian.net',
        email     'you@example.com',
        api_token 'your-api-token'
    );

-- 3. Map the account's tables. `object` selects which Jira table.
CREATE SCHEMA IF NOT EXISTS jira;

CREATE FOREIGN TABLE jira.projects (
    id   bigint,
    key  text,
    name text,
    lead text
) SERVER jira_account OPTIONS (object 'projects');

CREATE FOREIGN TABLE jira.issues (
    key      text,
    summary  text,
    status   text,
    assignee text,
    project  text,
    created  text
) SERVER jira_account OPTIONS (object 'issues');

CREATE FOREIGN TABLE jira.users (
    account_id   text,
    display_name text,
    email        text,
    active       boolean
) SERVER jira_account OPTIONS (object 'users');

CREATE FOREIGN TABLE jira.worklogs (
    id                 bigint,
    issue_key          text,
    author             text,
    time_spent_seconds bigint,
    started            text
) SERVER jira_account OPTIONS (object 'worklogs');

-- 4. Query with plain SQL. The WHERE clause is pushed down to Jira as JQL.
SELECT key, name, lead FROM jira.projects LIMIT 5;

SELECT key, summary, status, assignee
FROM jira.issues
WHERE project = 'ENG'
LIMIT 5;
