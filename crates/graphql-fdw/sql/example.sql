-- Example: query a GraphQL API from PostgreSQL via the generic GraphQL FDW.
--
-- The server's `spec` option carries a serialized GraphQlSpec — hand-written or
-- generated from schema introspection (see graphql-connector). Here it's the
-- public Countries API (countries.trevorblades.com), no auth. Foreign-table
-- columns are matched to the spec's columns by name.

CREATE EXTENSION IF NOT EXISTS graphql_fdw;

CREATE FOREIGN DATA WRAPPER budbuk_graphql
    HANDLER graphql_fdw_handler
    VALIDATOR graphql_fdw_validator;

CREATE SERVER countries FOREIGN DATA WRAPPER budbuk_graphql
    OPTIONS (spec '{"name":"countries","endpoint":"https://countries.trevorblades.com/","auth":{"type":"none"},"tables":[{"name":"countries","query":"query { countries { code name emoji continent { code name } } }","data_pointer":"/countries","shape":"list","columns":[{"name":"code","field":"code","data_type":"Text"},{"name":"name","field":"name","data_type":"Text"},{"name":"emoji","field":"emoji","data_type":"Text"},{"name":"continent","field":"continent","data_type":"Json"}],"pagination":{"style":"none"},"filters":[]}]}');

CREATE SCHEMA gql;
CREATE FOREIGN TABLE gql.countries (
    code text,
    name text,
    emoji text,
    continent text
) SERVER countries OPTIONS (object 'countries');

-- The nested `continent` object arrives as JSON text.
SELECT code, name, emoji, continent
FROM gql.countries
ORDER BY name
LIMIT 5;
