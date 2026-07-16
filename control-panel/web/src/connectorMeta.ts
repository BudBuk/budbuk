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
