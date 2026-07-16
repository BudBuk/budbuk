# BudBuk Connector Roadmap & Tracker

A living tracker of connectors for BudBuk, prioritized into tiers of ten. Jira is
shipped; everything else is backlog. Update the **Status** as work progresses.

## Status legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Done — shipped and working |
| 🚧 | In progress |
| 📋 | Next up — prioritized, not started |
| ⬜ | Backlog — not picked up yet |

**Progress:** Shipped — Jira, GitHub, Stripe, GitLab, Zendesk, PagerDuty,
Freshdesk, Contentful, **Asana, Shopify, Intercom, Pipedrive, and Sentry** (13
sources, all out-of-the-box via the catalog), the config-driven REST engine +
OpenAPI importer, a **GraphQL** engine + generator + FDW, and generic REST/GraphQL
FDWs with a **connector catalog** (`crates/catalog`).

> **The model:** standard connectors are out-of-the-box — their `SourceSpec` is
> bundled in the code and registered in the catalog, so they mount like Jira
> (`OPTIONS (connector 'stripe', api_key '…')`), no spec required. Marking one ✅
> below means "bundle its spec + add a catalog entry"; the engine, FDW, caching,
> pushdown, and observability are already done.

## How connectors are prioritized

1. **Ubiquity** — how many organizations actually use it.
2. **FDW fit** — clean REST/GraphQL with filterable params, so `WHERE`/`ORDER BY`/`LIMIT`
   push down well (like Jira → JQL).
3. **Analytics value** — tabular, read-heavy data worth joining in SQL.
4. **Reuse leverage** — shares auth/pagination with something already built, or
   unlocks siblings cheaply (see [Reuse clusters](#reuse-clusters)).

> **Highest-leverage items are the meta-connectors** (generic REST/OpenAPI, generic
> SQL database, generic GraphQL). Each unlocks dozens of sources with almost no
> per-source code — build them early.

---

## Shipped

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 0 | ✅ | **Jira** | Project mgmt | API token (Basic) | projects, issues, users, worklogs; JQL pushdown; caching; FDW |

---

## Tier 1 — build next (universal, high ROI, great API)

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 1 | ✅ | **GitHub** | Dev | PAT / public | `github-connector`: repos, issues, gists, orgs as a ~90-line `SourceSpec` over the REST engine — no HTTP code |
| 2 | ⬜ | **Salesforce** | CRM | OAuth2 | #1 CRM; SOQL → clean pushdown |
| 3 | ⬜ | **Google Sheets** | Productivity | OAuth2 | Universal; in the original spec |
| 4 | ✅ | **Stripe** | Payments | API key (Bearer) | Imports directly from Stripe's official OpenAPI — 104 tables, cursor pagination auto-detected. No hand-written code; add a test key for live queries |
| 5 | ✅ | **Slack** | Comms | OAuth2 (bot) | Near-universal |
| 6 | ✅ | **HubSpot** | CRM/Marketing | OAuth2 / private-app token | Dominant mid-market |
| 7 | ⬜ | **Google Analytics 4** | Analytics | OAuth2 | Near-universal web analytics |
| 8 | ✅ | **Zendesk** | Support | API token / OAuth2 | Clean, filterable API |
| 9 | ✅ | **Shopify** | E-commerce | Access token / OAuth2 | E-commerce leader |
| 10 | ⬜ | **Notion** | Docs/Knowledge | OAuth2 / integration token | Exploding adoption |

## Tier 2 — very common, strong demand

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 11 | ✅ | **Generic REST / OpenAPI** | Meta | Configurable | `rest-connector`: any API via a `SourceSpec`, hand-written or generated from an OpenAPI doc (`SourceSpec::from_openapi`) |
| 12 | ⬜ | **Generic SQL database** | Database | DB credentials | Postgres/MySQL/etc. as a source |
| 13 | ✅ | **GitLab** | Dev | OAuth2 / PAT | GitHub sibling |
| 14 | ⬜ | **Google Ads** | Ads | OAuth2 + developer token | Core marketing analytics |
| 15 | ⬜ | **Meta (Facebook/Instagram) Ads** | Ads | OAuth2 | Core marketing analytics |
| 16 | ⬜ | **Airtable** | Productivity/DB | PAT / OAuth2 | Spreadsheet-DB |
| 17 | ✅ | **Intercom** | Support/Messaging | OAuth2 / token | Support + product data |
| 18 | ⬜ | **QuickBooks Online** | Accounting | OAuth2 | Dominant SMB accounting |
| 19 | ✅ | **Asana** | Work mgmt | OAuth2 / PAT | Jira-adjacent |
| 20 | ⬜ | **Snowflake** | Data warehouse | key-pair / user-pass | Query the warehouse |

## Tier 3 — widespread, high analytics value

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 21 | ⬜ | **BigQuery** | Warehouse | OAuth2 / service account | Snowflake sibling |
| 22 | ⬜ | **Amazon S3 (+ CSV/Parquet)** | Storage/Files | AWS IAM keys | Universal file data |
| 23 | ✅ | **Confluence** | Docs | API token (Atlassian) | Reuses Jira auth |
| 24 | ✅ | **Mailchimp** | Email marketing | API key / OAuth2 | Ubiquitous SMB email |
| 25 | ⬜ | **Linear** | Dev/Issues | API key / OAuth2 | Modern GraphQL |
| 26 | ⬜ | **Xero** | Accounting | OAuth2 | QuickBooks alternative (intl.) |
| 27 | ⬜ | **Segment** | CDP | API token | Event/customer hub |
| 28 | ⬜ | **Datadog** | Observability | API + app keys | Metrics/monitors/logs |
| 29 | ⬜ | **Monday.com** | Work mgmt | API token (GraphQL) | Very common PM |
| 30 | ⬜ | **Amplitude** | Product analytics | API key + secret | Product analytics |

## Tier 4 — broad reach, category leaders

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 31 | ⬜ | **Mixpanel** | Product analytics | service account / secret | Amplitude sibling |
| 32 | ✅ | **PayPal** | Payments | OAuth2 | Stripe alternative |
| 33 | ⬜ | **Microsoft Teams** | Comms | OAuth2 (Graph) | Enterprise Slack alt |
| 34 | ✅ | **Zoom** | Meetings | OAuth2 (S2S) | Usage/reporting data |
| 35 | ✅ | **Freshdesk** | Support | API key | Zendesk alternative |
| 36 | ⬜ | **Klaviyo** | E-comm marketing | API key / OAuth2 | Dominant Shopify-era email |
| 37 | ✅ | **Pipedrive** | CRM | API token / OAuth2 | Popular SMB CRM |
| 38 | ✅ | **PagerDuty** | Incident mgmt | API token | On-call/incidents |
| 39 | ✅ | **ServiceNow** | ITSM/Enterprise | Basic / OAuth2 | Enterprise workflows |
| 40 | ⬜ | **NetSuite** | ERP | OAuth2 (token-based) | Mid-market/enterprise ERP |

## Tier 5 — commerce, billing, ads breadth

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 41 | ✅ | **WooCommerce** | E-commerce | API key/secret | WordPress commerce |
| 42 | ✅ | **BigCommerce** | E-commerce | API token | Shopify alternative |
| 43 | ⬜ | **Amazon Selling Partner (SP-API)** | E-commerce | LWA OAuth2 + AWS | Marketplace sellers |
| 44 | ✅ | **Square** | Payments/POS | OAuth2 / token | SMB payments |
| 45 | ✅ | **Chargebee** | Subscription billing | API key | SaaS revenue |
| 46 | ✅ | **Recurly** | Subscription billing | API key | Chargebee sibling |
| 47 | ⬜ | **LinkedIn Ads** | Ads | OAuth2 | B2B marketing |
| 48 | ⬜ | **TikTok Ads** | Ads | OAuth2 | Fast-growing |
| 49 | ⬜ | **Microsoft/Bing Ads** | Ads | OAuth2 + developer token | Search ads #2 |
| 50 | ✅ | **Zoho CRM** | CRM | OAuth2 | Large SMB base |

## Tier 6 — dev, files, identity

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 51 | ✅ | **Sentry** | Error tracking | auth token | Dev/observability |
| 52 | ✅ | **Bitbucket** | Dev | OAuth2 / app password | Atlassian repo host |
| 53 | ⬜ | **Trello** | Work mgmt | API key + token | Atlassian boards |
| 54 | ⬜ | **ClickUp** | Work mgmt | OAuth2 / API token | Growing PM tool |
| 55 | ✅ | **Jira Service Management** | ITSM | API token (Atlassian) | Reuses Jira work |
| 56 | ⬜ | **Google Drive** | Files | OAuth2 | Universal file store |
| 57 | ⬜ | **Dropbox** | Files | OAuth2 | Common file store |
| 58 | ✅ | **Box** | Files | OAuth2 / JWT | Enterprise file store |
| 59 | ✅ | **Okta** | Identity | API token / OAuth2 | SSO/audit logs |
| 60 | ✅ | **Auth0** | Identity | OAuth2 (mgmt API) | Dev-centric identity |

## Tier 7 — marketing automation, forms, social

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 61 | ⬜ | **Marketo** | Marketing automation | OAuth2 (REST) | Enterprise MA |
| 62 | ⬜ | **Salesforce Pardot** | Marketing automation | OAuth2 | B2B MA |
| 63 | ✅ | **ActiveCampaign** | Marketing/CRM | API key | SMB MA |
| 64 | ✅ | **SendGrid** | Email delivery | API key | Transactional email |
| 65 | ✅ | **Typeform** | Forms | OAuth2 / PAT | Survey/lead data |
| 66 | ✅ | **SurveyMonkey** | Surveys | OAuth2 | Survey data |
| 67 | ⬜ | **Google Search Console** | SEO | OAuth2 | Search performance |
| 68 | ⬜ | **YouTube (Data/Analytics)** | Media | OAuth2 | Channel/video metrics |
| 69 | ⬜ | **Twitter / X** | Social | OAuth2 | Social listening |
| 70 | ⬜ | **Google Calendar** | Calendar | OAuth2 | Scheduling/meeting data |

## Tier 8 — data infra, enterprise CRM/HR

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 71 | ⬜ | **Redshift** | Warehouse | DB creds / IAM | AWS warehouse |
| 72 | ⬜ | **Databricks** | Lakehouse | PAT / OAuth2 | Growing fast |
| 73 | ⬜ | **MongoDB** | Database | connection string | Dominant NoSQL |
| 74 | ⬜ | **Elasticsearch / OpenSearch** | Search/DB | API key / basic | Logs & search |
| 75 | ⬜ | **ClickHouse** | Database | user/pass | Analytics DB, rising |
| 76 | ⬜ | **Microsoft SQL Server** | Database | DB creds | Enterprise RDBMS |
| 77 | ⬜ | **Microsoft Dynamics 365** | CRM/ERP | OAuth2 (Dataverse) | Enterprise MS stack |
| 78 | ⬜ | **Workday** | HR/Finance | OAuth2 / API | Enterprise HCM |
| 79 | ⬜ | **BambooHR** | HR | API key | SMB/mid HR |
| 80 | ✅ | **Greenhouse** | Recruiting | API key | ATS data |

## Tier 9 — HR/finance long tail, comms

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 81 | ✅ | **Lever** | Recruiting | API key / OAuth2 | Greenhouse sibling |
| 82 | ⬜ | **Gusto** | Payroll | OAuth2 | SMB payroll |
| 83 | ⬜ | **Rippling** | HR/IT | API key / OAuth2 | Fast-growing |
| 84 | ⬜ | **ADP** | Payroll | OAuth2 (certs) | Enterprise payroll |
| 85 | ✅ | **DocuSign** | E-signature | OAuth2 (JWT) | Contract data |
| 86 | ✅ | **Calendly** | Scheduling | OAuth2 / PAT | Booking data |
| 87 | ⬜ | **Gmail / Google Workspace** | Email | OAuth2 | Mailbox/audit data |
| 88 | ⬜ | **Microsoft 365 / Outlook** | Email/Calendar | OAuth2 (Graph) | Enterprise mail |
| 89 | ✅ | **Twilio** | Comms/SMS | API key (SID/token) | Messaging usage |
| 90 | ⬜ | **Braze** | Engagement | API key | Lifecycle messaging |

## Tier 10 — observability, CDP, specialized

| # | Status | Connector | Category | Auth | Notes |
|---|--------|-----------|----------|------|-------|
| 91 | ⬜ | **New Relic** | Observability | API key (NerdGraph) | APM data |
| 92 | ✅ | **Grafana / Prometheus** | Metrics | API key / basic | Metrics store |
| 93 | ⬜ | **Splunk** | Logs/SIEM | token / basic | Enterprise logs |
| 94 | ✅ | **Opsgenie** | Incident | API key | PagerDuty sibling (Atlassian) |
| 95 | ⬜ | **Zuora** | Billing | OAuth2 / token | Enterprise billing |
| 96 | ⬜ | **RudderStack** | CDP | access token | Segment alternative |
| 97 | ✅ | **Smartsheet** | Work mgmt | API token / OAuth2 | Enterprise spreadsheets |
| 98 | ⬜ | **Coda** | Docs | API token | Notion sibling |
| 99 | ⬜ | **Generic GraphQL** | Meta | Configurable | One connector for any GraphQL API |
| 100 | ⬜ | **Amazon Ads** | Ads | LWA OAuth2 | Retail media, growing |

---

## Reuse clusters

Build one, the siblings come nearly free (same auth + pagination + a swapped
pushdown dialect):

- **Atlassian:** Jira ✅ → Confluence, Bitbucket, Trello, Jira Service Management, Opsgenie
- **Google / OAuth2:** Sheets → Analytics 4, Ads, Search Console, Drive, Calendar, Gmail, YouTube
- **Ad platforms:** Google Ads → Meta, LinkedIn, TikTok, Bing, Amazon Ads (all "report + date range + breakdown")
- **Warehouses / DBs:** one generic SQL connector → Snowflake, BigQuery, Redshift, MySQL, SQL Server, ClickHouse
- **Product analytics:** Amplitude ↔ Mixpanel
- **Subscription billing:** Chargebee ↔ Recurly ↔ Zuora
- **Support:** Zendesk ↔ Freshdesk ↔ Intercom
- **Recruiting/HR:** Greenhouse ↔ Lever; Gusto ↔ Rippling ↔ ADP

## Recommended build order after Jira

1. **GitHub** — proves SDK reuse; closest to Jira.
2. **Generic REST / OpenAPI** — the force multiplier.
3. **Salesforce** or **Stripe** — highest-demand SaaS; builds the OAuth2 flow you'll reuse everywhere.

## Updating this tracker

When you pick up a connector, change its **Status** to 🚧, and to ✅ when it ships
(update the progress count at the top). Add tables/rows as new sources are requested.
