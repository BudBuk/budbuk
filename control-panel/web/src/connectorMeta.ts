// Presentation metadata for the connector catalog: category grouping,
// one-line descriptions, category colors, and monogram icons.
//
// This file is purely for the UI. The backend is the source of truth for
// which connectors exist and what options they take; anything here that is
// missing simply falls back to the "Other" category and a generic look.

export interface ConnectorMeta {
  category: string
  description: string
}

export const FALLBACK_CATEGORY = 'Other'

// Connectors grouped by category. Keeping the grouping as the source shape
// means each category string is written once, so CONNECTOR_META and the
// ordered category list below can be derived from it without drift.
const GROUPS: Record<string, Record<string, string>> = {
  'Dev & Issues': {
    github: 'Code hosting — repos, issues, pull requests',
    gitlab: 'DevOps platform — projects, issues, merge requests',
    bitbucket: 'Git hosting — repositories, pull requests, pipelines',
    jsm: 'Jira Service Management — requests, incidents, queues',
    sentry: 'Error monitoring — issues, events, releases',
  },
  'Support & ITSM': {
    zendesk: 'Customer support — tickets, users, organizations',
    freshdesk: 'Helpdesk — tickets, contacts, agents',
    intercom: 'Customer messaging — conversations, contacts, companies',
    servicenow: 'ITSM — incidents, changes, problems',
    pagerduty: 'Incident response — incidents, services, on-calls',
    opsgenie: 'Alerting & on-call — alerts, schedules, teams',
  },
  'CRM & Marketing': {
    hubspot: 'CRM — contacts, companies, deals',
    pipedrive: 'Sales CRM — deals, persons, activities',
    zohocrm: 'CRM — leads, contacts, deals',
    activecampaign: 'Marketing automation — contacts, campaigns, deals',
    mailchimp: 'Email marketing — lists, campaigns, subscribers',
    klaviyo: 'Marketing — profiles, lists, campaigns',
  },
  'Payments & Billing': {
    stripe: 'Payments — charges, customers, invoices',
    paypal: 'Payments — transactions, payouts, orders',
    square: 'Payments & POS — payments, orders, customers',
    chargebee: 'Subscription billing — subscriptions, invoices, customers',
    recurly: 'Subscription billing — accounts, subscriptions, invoices',
  },
  'E-commerce': {
    shopify: 'E-commerce — orders, products, customers',
    woocommerce: 'E-commerce — orders, products, customers',
    bigcommerce: 'E-commerce — orders, products, customers',
  },
  'Comms & Meetings': {
    slack: 'Team messaging — users, channels, usergroups',
    zoom: 'Video meetings — meetings, participants, recordings',
    twilio: 'Messaging & voice — messages, calls, phone numbers',
    calendly: 'Scheduling — events, invitees, event types',
  },
  'Work, Docs & CMS': {
    asana: 'Work management — tasks, projects, teams',
    monday: 'Work OS — boards, items, users, workspaces',
    smartsheet: 'Work management — sheets, rows, columns',
    notion: 'Docs & wiki — pages, databases, users',
    confluence: 'Team wiki — spaces, pages, content',
    contentful: 'Headless CMS — entries, assets, content types',
  },
  'Forms & Email': {
    typeform: 'Forms & surveys — forms, responses, answers',
    surveymonkey: 'Surveys — surveys, responses, collectors',
    sendgrid: 'Email delivery — messages, contacts, stats',
  },
  'Identity & Files': {
    okta: 'Identity — users, groups, applications',
    auth0: 'Identity — users, connections, clients',
    box: 'File storage — files, folders, users',
    gdrive: 'Google Drive — files, folders, permissions',
    msgraph: 'Microsoft 365 — users, groups, messages',
  },
  Observability: {
    datadog: 'Monitoring — metrics, monitors, events',
    grafana: 'Dashboards & alerts — dashboards, alerts, data sources',
  },
  'HR & Recruiting': {
    greenhouse: 'Recruiting — candidates, jobs, applications',
    lever: 'Recruiting — candidates, postings, opportunities',
  },
  'Finance & Other': {
    xero: 'Accounting — invoices, contacts, payments',
    docusign: 'E-signature — envelopes, recipients, documents',
    gcalendar: 'Google Calendar — calendars, events, attendees',
  },
  'AI & ML': {
    huggingface: 'ML hub — models, datasets, spaces',
    granola: 'AI meeting notes — notes',
  },
  Meta: {
    openapi: 'Generic OpenAPI — any spec-defined endpoints',
  },
}

// Flattened lookup: connector name -> { category, description }.
export const CONNECTOR_META: Record<string, ConnectorMeta> = (() => {
  const out: Record<string, ConnectorMeta> = {}
  for (const [category, conns] of Object.entries(GROUPS)) {
    for (const [name, description] of Object.entries(conns)) {
      out[name] = { category, description }
    }
  }
  return out
})()

// Ordered list of categories as declared above (used for filter chips).
export const CATEGORIES: string[] = Object.keys(GROUPS)

// A distinct, tasteful color per category. The palette is anchored on the
// BudBuk brand blue (#1E4E86) and orange (#F6883B); the remaining hues are
// chosen to stay legible with white text on top.
export const CATEGORY_COLOR: Record<string, string> = {
  'Dev & Issues': '#1E4E86',
  'Support & ITSM': '#0E7490',
  'CRM & Marketing': '#F6883B',
  'Payments & Billing': '#2E9E5B',
  'E-commerce': '#C2410C',
  'Comms & Meetings': '#6D4AC4',
  'Work, Docs & CMS': '#3B6FB0',
  'Forms & Email': '#BE4B8A',
  'Identity & Files': '#475569',
  Observability: '#9333EA',
  'HR & Recruiting': '#10897B',
  'Finance & Other': '#B7791F',
  'AI & ML': '#4F46E5',
  Meta: '#64748B',
  [FALLBACK_CATEGORY]: '#94A3B8',
}

// Metadata for a connector, falling back to the "Other" category for any
// connector the backend exposes that we do not have curated copy for.
export function metaFor(name: string): ConnectorMeta {
  return (
    CONNECTOR_META[name] ?? {
      category: FALLBACK_CATEGORY,
      description: 'Custom connector',
    }
  )
}

// Color for a category, falling back to grey for unknown categories.
export function categoryColor(category: string): string {
  return CATEGORY_COLOR[category] ?? CATEGORY_COLOR[FALLBACK_CATEGORY]
}

// A few brands read better as their well-known two-letter mark than as their
// first two letters (e.g. GitHub -> "GH", not "GI").
const MONOGRAM_OVERRIDES: Record<string, string> = {
  github: 'GH',
  gitlab: 'GL',
  bitbucket: 'BB',
  servicenow: 'SN',
  pagerduty: 'PD',
  opsgenie: 'OG',
  activecampaign: 'AC',
  zohocrm: 'ZC',
  woocommerce: 'WC',
  bigcommerce: 'BC',
  surveymonkey: 'SM',
  sendgrid: 'SG',
  freshdesk: 'FD',
  gdrive: 'GD',
  gcalendar: 'GC',
  msgraph: 'MS',
}

// One or two uppercase letters used inside the standardized square icon.
export function monogram(name: string): string {
  const key = name.toLowerCase()
  const override = MONOGRAM_OVERRIDES[key]
  if (override) return override
  const letters = key.replace(/[^a-z0-9]/g, '')
  if (letters.length === 0) return '?'
  return letters.slice(0, 2).toUpperCase()
}

// Map from connector name -> simple-icons slug, so cards and modals can show
// the real brand logo via https://cdn.simpleicons.org/<slug>. Connectors not
// listed here (or whose image fails to load) fall back to the monogram chip.
export const SLUG: Record<string, string> = {
  stripe: 'stripe',
  github: 'github',
  gitlab: 'gitlab',
  slack: 'slack',
  zendesk: 'zendesk',
  pagerduty: 'pagerduty',
  contentful: 'contentful',
  asana: 'asana',
  shopify: 'shopify',
  intercom: 'intercom',
  pipedrive: 'pipedrive',
  sentry: 'sentry',
  hubspot: 'hubspot',
  mailchimp: 'mailchimp',
  zoom: 'zoom',
  okta: 'okta',
  auth0: 'auth0',
  twilio: 'twilio',
  typeform: 'typeform',
  calendly: 'calendly',
  bitbucket: 'bitbucket',
  square: 'square',
  confluence: 'confluence',
  woocommerce: 'woocommerce',
  activecampaign: 'activecampaign',
  surveymonkey: 'surveymonkey',
  paypal: 'paypal',
  box: 'box',
  grafana: 'grafana',
  klaviyo: 'klaviyo',
  datadog: 'datadog',
  xero: 'xero',
  notion: 'notion',
  monday: 'mondaydotcom',
  huggingface: 'huggingface',
  docusign: 'docusign',
  sendgrid: 'sendgrid',
  greenhouse: 'greenhouse',
  smartsheet: 'smartsheet',
  // Brand slug differs from the connector name we use internally.
  gdrive: 'googledrive',
  gcalendar: 'googlecalendar',
  zohocrm: 'zoho',
  jsm: 'jira',
  servicenow: 'servicenow',
  freshdesk: 'freshdesk',
}

// The simple-icons slug for a connector, or null if we don't have one.
export function slugFor(name: string): string | null {
  return SLUG[name.toLowerCase()] ?? null
}

// Proper brand casing for display. The raw (lowercase) id is still what the
// backend expects for API calls; this is only for what the user reads.
export const DISPLAY_NAME: Record<string, string> = {
  stripe: 'Stripe',
  github: 'GitHub',
  gitlab: 'GitLab',
  zendesk: 'Zendesk',
  pagerduty: 'PagerDuty',
  freshdesk: 'Freshdesk',
  contentful: 'Contentful',
  asana: 'Asana',
  shopify: 'Shopify',
  intercom: 'Intercom',
  pipedrive: 'Pipedrive',
  sentry: 'Sentry',
  hubspot: 'HubSpot',
  slack: 'Slack',
  mailchimp: 'Mailchimp',
  zoom: 'Zoom',
  servicenow: 'ServiceNow',
  okta: 'Okta',
  auth0: 'Auth0',
  twilio: 'Twilio',
  typeform: 'Typeform',
  opsgenie: 'Opsgenie',
  smartsheet: 'Smartsheet',
  calendly: 'Calendly',
  bitbucket: 'Bitbucket',
  square: 'Square',
  recurly: 'Recurly',
  confluence: 'Confluence',
  woocommerce: 'WooCommerce',
  bigcommerce: 'BigCommerce',
  zohocrm: 'Zoho CRM',
  activecampaign: 'ActiveCampaign',
  surveymonkey: 'SurveyMonkey',
  sendgrid: 'SendGrid',
  greenhouse: 'Greenhouse',
  lever: 'Lever',
  chargebee: 'Chargebee',
  paypal: 'PayPal',
  docusign: 'DocuSign',
  box: 'Box',
  jsm: 'Jira Service Management',
  grafana: 'Grafana',
  klaviyo: 'Klaviyo',
  datadog: 'Datadog',
  xero: 'Xero',
  msgraph: 'Microsoft Graph',
  gdrive: 'Google Drive',
  gcalendar: 'Google Calendar',
  notion: 'Notion',
  monday: 'Monday.com',
  huggingface: 'Hugging Face',
  granola: 'Granola',
  openapi: 'OpenAPI',
}

// Proper brand casing, falling back to capitalizing the first letter.
export function displayName(name: string): string {
  const key = name.toLowerCase()
  const known = DISPLAY_NAME[key]
  if (known) return known
  return name.charAt(0).toUpperCase() + name.slice(1)
}

// Marketing/home page per connector, for the external-link affordance.
export const WEBSITE: Record<string, string> = {
  stripe: 'https://stripe.com',
  github: 'https://github.com',
  gitlab: 'https://gitlab.com',
  zendesk: 'https://www.zendesk.com',
  pagerduty: 'https://www.pagerduty.com',
  freshdesk: 'https://www.freshworks.com/freshdesk/',
  contentful: 'https://www.contentful.com',
  asana: 'https://asana.com',
  shopify: 'https://www.shopify.com',
  intercom: 'https://www.intercom.com',
  pipedrive: 'https://www.pipedrive.com',
  sentry: 'https://sentry.io',
  hubspot: 'https://www.hubspot.com',
  slack: 'https://slack.com',
  mailchimp: 'https://mailchimp.com',
  zoom: 'https://zoom.us',
  servicenow: 'https://www.servicenow.com',
  okta: 'https://www.okta.com',
  auth0: 'https://auth0.com',
  twilio: 'https://www.twilio.com',
  typeform: 'https://www.typeform.com',
  opsgenie: 'https://www.atlassian.com/software/opsgenie',
  smartsheet: 'https://www.smartsheet.com',
  calendly: 'https://calendly.com',
  bitbucket: 'https://bitbucket.org',
  square: 'https://squareup.com',
  recurly: 'https://recurly.com',
  confluence: 'https://www.atlassian.com/software/confluence',
  woocommerce: 'https://woocommerce.com',
  bigcommerce: 'https://www.bigcommerce.com',
  zohocrm: 'https://www.zoho.com/crm/',
  activecampaign: 'https://www.activecampaign.com',
  surveymonkey: 'https://www.surveymonkey.com',
  sendgrid: 'https://sendgrid.com',
  greenhouse: 'https://www.greenhouse.io',
  lever: 'https://www.lever.co',
  chargebee: 'https://www.chargebee.com',
  paypal: 'https://www.paypal.com',
  docusign: 'https://www.docusign.com',
  box: 'https://www.box.com',
  jsm: 'https://www.atlassian.com/software/jira/service-management',
  grafana: 'https://grafana.com',
  klaviyo: 'https://www.klaviyo.com',
  datadog: 'https://www.datadoghq.com',
  xero: 'https://www.xero.com',
  msgraph: 'https://developer.microsoft.com/graph',
  gdrive: 'https://www.google.com/drive/',
  gcalendar: 'https://calendar.google.com',
  notion: 'https://www.notion.so',
  monday: 'https://monday.com',
  huggingface: 'https://huggingface.co',
  granola: 'https://granola.ai',
  openapi: 'https://www.openapis.org',
}

// Homepage URL for a connector, or null if we don't have one.
export function websiteFor(name: string): string | null {
  return WEBSITE[name.toLowerCase()] ?? null
}
