# Configuring BudBuk connectors in PostgreSQL

This is the complete reference for mounting BudBuk's connectors as PostgreSQL
foreign tables. Standard connectors are **out-of-the-box** — their spec ships in
BudBuk, so you provide only a name and credentials, exactly like Jira.

- [Install the extensions](#1-install-the-extensions)
- [Out-of-the-box connectors](#2-out-of-the-box-connectors)
- [Jira, and multiple accounts](#3-jira--and-multiple-accounts)
- [Foreign tables](#4-foreign-tables)
- [Querying across accounts and connectors](#5-querying)
- [The isolation model](#the-isolation-model)
- [Credentials & security](#credentials--security)
- [Option reference](#option-reference)

## 1. Install the extensions

Once per database:

```sql
CREATE EXTENSION rest_fdw;   -- generic engine: every catalog + OpenAPI connector
CREATE EXTENSION jira_fdw;   -- Jira's dedicated FDW (native JQL pushdown)

CREATE FOREIGN DATA WRAPPER budbuk HANDLER rest_fdw_handler VALIDATOR rest_fdw_validator;
CREATE FOREIGN DATA WRAPPER jira   HANDLER jira_fdw_handler VALIDATOR jira_fdw_validator;
```

## 2. Out-of-the-box connectors

Pick a built-in connector by name and give only its credentials/config — the
`SourceSpec` is bundled in BudBuk's [catalog](../crates/catalog).

```sql
-- Stripe: only an API key
CREATE SERVER stripe FOREIGN DATA WRAPPER budbuk
    OPTIONS (connector 'stripe', api_key 'sk_live_…');

-- GitHub: owner/repo (+ optional token for private repos / higher rate limits)
CREATE SERVER github FOREIGN DATA WRAPPER budbuk
    OPTIONS (connector 'github', owner 'acme', repo 'app', token 'ghp_…');
```

**The long tail** — any REST API with an OpenAPI document, no bundled connector
required:

```sql
CREATE SERVER my_api FOREIGN DATA WRAPPER budbuk
    OPTIONS (connector 'openapi', spec '…openapi json…', token '…');
```

## 3. Jira — and multiple accounts

Jira uses its own FDW. **One server = one account (one Jira site).** Need several
accounts? Create several servers — each is fully independent.

```sql
-- Account A: your company Jira
CREATE SERVER jira_work FOREIGN DATA WRAPPER jira
    OPTIONS (base_url 'https://acme.atlassian.net',
             email 'you@acme.com', api_token 'ATATT_A…');

-- Account B: a client / second org — its own URL, token, and cache
CREATE SERVER jira_side FOREIGN DATA WRAPPER jira
    OPTIONS (base_url 'https://client.atlassian.net',
             email 'you@client.com', api_token 'ATATT_B…');
```

The same pattern applies to any connector: two Stripe accounts (test + live)?
Two servers, each `connector 'stripe'`. Three GitHub orgs? Three servers.

## 4. Foreign tables

One schema per source keeps table names clean; the `object` option selects which
of the connector's tables to expose.

```sql
CREATE SCHEMA stripe;  CREATE SCHEMA gh;  CREATE SCHEMA work;  CREATE SCHEMA side;

CREATE FOREIGN TABLE stripe.charges (id text, amount bigint, status text, customer text)
    SERVER stripe OPTIONS (object 'charges');
CREATE FOREIGN TABLE gh.repos (name text, stars bigint, language text)
    SERVER github OPTIONS (object 'repos');
CREATE FOREIGN TABLE work.issues (key text, summary text, status text, project text)
    SERVER jira_work OPTIONS (object 'issues');   -- account A
CREATE FOREIGN TABLE side.issues (key text, summary text, status text, project text)
    SERVER jira_side OPTIONS (object 'issues');   -- account B
```

You only declare the columns you care about; each maps by name to a field the
connector exposes. Timestamps are exposed as `text`.

## 5. Querying

Across **both Jira accounts** in one statement:

```sql
SELECT 'acme'   AS account, key, status FROM work.issues WHERE status = 'Open'
UNION ALL
SELECT 'client' AS account, key, status FROM side.issues WHERE status = 'Open';
```

Across **connectors** — enrich your own users with Stripe spend:

```sql
SELECT u.email, coalesce(sum(ch.amount) FILTER (WHERE ch.status='succeeded'),0)/100.0 AS usd
FROM app_users u
LEFT JOIN stripe.customers c ON c.email = u.email
LEFT JOIN stripe.charges  ch ON ch.customer = c.id
GROUP BY u.email;
```

`WHERE` clauses on filterable columns push down to the source API (e.g. Jira
`WHERE project = 'ENG'` → JQL `project = "ENG"`; Stripe `WHERE customer = 'cus_x'`
→ `?customer=cus_x`). Aggregates, `ORDER BY`, joins, and non-pushable filters run
in Postgres.

## The isolation model

Each `CREATE SERVER` is an independent account:

- **Credentials** are per-server — account A's token is never used for B.
- **Cache** keys are namespaced by the account (its `base_url`), so A's cached
  rows and B's never mix, even for byte-identical queries.
- **Rate-limit** state is per-account.

## Credentials & security

For this proof of concept, credentials live in `SERVER OPTIONS`, which are visible
to superusers in the catalogs (`pg_foreign_server`). For production:

- Put secrets in **`CREATE USER MAPPING FOR <role> … OPTIONS (api_token '…')`** so
  they're scoped per database role and protected from ordinary users, **or**
- Source them from a secrets manager (roadmap).

Never commit real keys. Rotate any key that has been shared in plaintext.

## Option reference

| Connector | Wrapper | Required options | Optional options |
|-----------|---------|------------------|------------------|
| `stripe`  | `budbuk` | `connector 'stripe'`, `api_key` | `base_url` |
| `github`  | `budbuk` | `connector 'github'`, `owner` | `repo`, `token`, `base_url` |
| `openapi` | `budbuk` | `connector 'openapi'`, `spec` | `token` / `api_key`, `base_url` |
| *(raw)*   | `budbuk` | `spec` (a serialized `SourceSpec`) | — |
| Jira      | `jira`   | `base_url`, `email`, `api_token` | — |

All foreign tables take `OPTIONS (object '<table>')` to choose the connector table.
