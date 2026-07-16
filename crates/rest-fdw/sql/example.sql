-- Example: query built-in connectors from PostgreSQL via the generic REST FDW.
--
-- Standard connectors are out-of-the-box — their spec is bundled in BudBuk, so
-- you mount them with just a name + credentials, exactly like Jira. The
-- connector catalog (crates/catalog) resolves the name to a bundled SourceSpec.

CREATE EXTENSION IF NOT EXISTS rest_fdw;

CREATE FOREIGN DATA WRAPPER budbuk
    HANDLER rest_fdw_handler
    VALIDATOR rest_fdw_validator;

-- ── Stripe: just an API key ────────────────────────────────────────────────
CREATE SERVER stripe FOREIGN DATA WRAPPER budbuk
    OPTIONS (connector 'stripe', api_key 'sk_live_...');
CREATE SCHEMA stripe;
CREATE FOREIGN TABLE stripe.charges (
    id text, amount bigint, currency text, status text, customer text, created bigint
) SERVER stripe OPTIONS (object 'charges');
CREATE FOREIGN TABLE stripe.customers (
    id text, name text, email text
) SERVER stripe OPTIONS (object 'customers');

SELECT round(sum(amount)/100.0, 2) AS revenue_usd
FROM stripe.charges WHERE status = 'succeeded';

-- ── GitHub: owner/repo (+ optional token) ──────────────────────────────────
CREATE SERVER gh FOREIGN DATA WRAPPER budbuk
    OPTIONS (connector 'github', owner 'octocat', repo 'Hello-World');
CREATE SCHEMA gh;
CREATE FOREIGN TABLE gh.repos (name text, stars bigint, language text)
    SERVER gh OPTIONS (object 'repos');

SELECT name, stars FROM gh.repos ORDER BY stars DESC LIMIT 5;

-- ── Zero-DDL: auto-mount every table with IMPORT FOREIGN SCHEMA ─────────────
-- Instead of hand-writing CREATE FOREIGN TABLE, let BudBuk create them from the
-- connector's discovered schema. Honors LIMIT TO / EXCEPT.
CREATE SERVER gl FOREIGN DATA WRAPPER budbuk OPTIONS (connector 'gitlab');
CREATE SCHEMA gl;
IMPORT FOREIGN SCHEMA gitlab FROM SERVER gl INTO gl;
--   → auto-creates gl.projects, gl.issues, gl.users (typed columns)
--   IMPORT FOREIGN SCHEMA gitlab LIMIT TO (projects) FROM SERVER gl INTO gl;

SELECT path_with_namespace, visibility FROM gl.projects LIMIT 5;

-- ── The long tail: bring your own OpenAPI document ─────────────────────────
-- CREATE SERVER myapi FOREIGN DATA WRAPPER budbuk
--     OPTIONS (connector 'openapi', spec '...openapi json...', token '...');
