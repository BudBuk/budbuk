import {
  siStripe,
  siGithub,
  siGitlab,
  siZendesk,
  siPagerduty,
  siContentful,
  siAsana,
  siShopify,
  siIntercom,
  siSentry,
  siHubspot,
  siMailchimp,
  siZoom,
  siOkta,
  siAuth0,
  siTypeform,
  siCalendly,
  siBitbucket,
  siSquare,
  siConfluence,
  siWoocommerce,
  siSurveymonkey,
  siPaypal,
  siBox,
  siGrafana,
  siDatadog,
  siXero,
  siNotion,
  siGreenhouse,
  siGoogledrive,
  siGooglecalendar,
  siZoho,
  siJira,
  siHuggingface,
} from 'simple-icons'
import { categoryColor, metaFor, monogram, slugFor } from '../connectorMeta'

interface Props {
  name: string
  size?: number
}

interface BrandIcon {
  path: string
  hex: string
}

// Static map of BUNDLED simple-icons, keyed by the simple-icons slug that our
// SLUG map resolves connector ids to. These are bundled at build time — no
// runtime CDN, works offline. Connectors whose slug is absent here (or that
// have no slug at all) fall back to the category-colored monogram chip below.
const ICONS: Record<string, BrandIcon> = {
  stripe: siStripe,
  github: siGithub,
  gitlab: siGitlab,
  zendesk: siZendesk,
  pagerduty: siPagerduty,
  contentful: siContentful,
  asana: siAsana,
  shopify: siShopify,
  intercom: siIntercom,
  sentry: siSentry,
  hubspot: siHubspot,
  mailchimp: siMailchimp,
  zoom: siZoom,
  okta: siOkta,
  auth0: siAuth0,
  typeform: siTypeform,
  calendly: siCalendly,
  bitbucket: siBitbucket,
  square: siSquare,
  confluence: siConfluence,
  woocommerce: siWoocommerce,
  surveymonkey: siSurveymonkey,
  paypal: siPaypal,
  box: siBox,
  grafana: siGrafana,
  datadog: siDatadog,
  xero: siXero,
  notion: siNotion,
  greenhouse: siGreenhouse,
  googledrive: siGoogledrive,
  googlecalendar: siGooglecalendar,
  zoho: siZoho,
  jira: siJira,
  huggingface: siHuggingface,
}

// Shows the real, bundled brand logo when available, otherwise the
// category-colored monogram chip. It never renders a blank square.
export default function BrandLogo({ name, size = 40 }: Props) {
  const slug = slugFor(name)
  const icon = slug ? ICONS[slug] : undefined

  if (!icon) {
    const color = categoryColor(metaFor(name).category)
    return (
      <span
        className="brand-logo brand-logo-mono"
        style={{ width: size, height: size, background: color }}
        aria-hidden="true"
      >
        {monogram(name)}
      </span>
    )
  }

  const glyph = Math.round(size * 0.56)

  return (
    <span className="brand-logo" style={{ width: size, height: size }} aria-hidden="true">
      <svg
        width={glyph}
        height={glyph}
        viewBox="0 0 24 24"
        role="img"
        fill={`#${icon.hex}`}
        xmlns="http://www.w3.org/2000/svg"
      >
        <path d={icon.path} />
      </svg>
    </span>
  )
}
