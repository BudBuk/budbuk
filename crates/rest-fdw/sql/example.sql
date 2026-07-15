-- Example: query GitHub from PostgreSQL via the generic REST FDW.
--
-- `rest_fdw` is driven entirely by a `spec` option — a serialized BudBuk
-- SourceSpec. Any connector (GitHub, JSONPlaceholder, an OpenAPI import) becomes
-- SQL-queryable through this one extension.
--
-- Generate the GitHub spec JSON with:
--     cargo run -p github-connector --example print_spec
-- then paste it into the `spec` option below. (For private/authenticated
-- access, set GITHUB_TOKEN before generating the spec; keep the resulting JSON
-- secret, since it then contains the token.)

CREATE EXTENSION IF NOT EXISTS rest_fdw;

CREATE FOREIGN DATA WRAPPER rest_wrapper
    HANDLER rest_fdw_handler
    VALIDATOR rest_fdw_validator;

CREATE SERVER github
    FOREIGN DATA WRAPPER rest_wrapper
    OPTIONS (spec '<PASTE SourceSpec JSON HERE>');

CREATE SCHEMA gh;

CREATE FOREIGN TABLE gh.repos (
    id          bigint,
    name        text,
    full_name   text,
    private     boolean,
    language    text,
    stars       bigint,
    forks       bigint,
    owner       text,
    description text,
    created     text,
    updated     text
) SERVER github OPTIONS (object 'repos');

CREATE FOREIGN TABLE gh.issues (
    number   bigint,
    title    text,
    state    text,
    "user"   text,   -- quoted: "user" is a reserved word
    comments bigint,
    created  text,
    updated  text
) SERVER github OPTIONS (object 'issues');

-- Query with plain SQL. `WHERE state = '...'` on issues is pushed down to
-- GitHub as `?state=...`; ORDER BY / aggregates / other filters run in Postgres.
SELECT name, stars, forks FROM gh.repos ORDER BY stars DESC LIMIT 5;
SELECT number, "user", title FROM gh.issues WHERE state = 'open' LIMIT 5;
SELECT count(*) AS repos, sum(stars) AS total_stars FROM gh.repos;
