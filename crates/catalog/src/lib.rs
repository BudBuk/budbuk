//! # catalog
//!
//! The built-in connector catalog. Given a connector *name* and the server's
//! options, it produces a ready-to-run [`SourceSpec`]. This is what lets
//! standard connectors mount out-of-the-box — the caller supplies only
//! credentials/config, and the bundled spec does the rest:
//!
//! ```text
//! CREATE SERVER stripe OPTIONS (connector 'stripe', api_key 'sk_...');
//! CREATE SERVER gh     OPTIONS (connector 'github', owner 'acme', repo 'app');
//! CREATE SERVER myapi  OPTIONS (connector 'openapi', spec '...', token '...');
//! ```
//!
//! Adding a new standard connector = bundle its spec and add one match arm.

pub mod options;
pub use options::{options_for, OptionSpec};

use std::collections::HashMap;

use activecampaign_connector::{activecampaign_spec, ActiveCampaignConfig};
use asana_connector::{asana_spec, AsanaConfig};
use auth0_connector::{auth0_spec, Auth0Config};
use bigcommerce_connector::{bigcommerce_spec, BigCommerceConfig};
use bitbucket_connector::{bitbucket_spec, BitbucketConfig};
use box_connector::{box_spec, BoxConfig};
use calendly_connector::{calendly_spec, CalendlyConfig};
use chargebee_connector::{chargebee_spec, ChargebeeConfig};
use confluence_connector::{confluence_spec, ConfluenceConfig};
use contentful_connector::{contentful_spec, ContentfulConfig};
use datadog_connector::{datadog_spec, DatadogConfig};
use docusign_connector::{docusign_spec, DocusignConfig};
use freshdesk_connector::{freshdesk_spec, FreshdeskConfig};
use gcalendar_connector::{gcalendar_spec, GcalendarConfig};
use gdrive_connector::{gdrive_spec, GdriveConfig};
use github_connector::{github_spec, GithubConfig};
use gitlab_connector::{gitlab_spec, GitLabConfig};
use grafana_connector::{grafana_spec, GrafanaConfig};
use granola_connector::{granola_spec, GranolaConfig};
use greenhouse_connector::{greenhouse_spec, GreenhouseConfig};
use hubspot_connector::{hubspot_spec, HubspotConfig};
use huggingface_connector::{huggingface_spec, HuggingFaceConfig};
use intercom_connector::{intercom_spec, IntercomConfig};
use jsm_connector::{jsm_spec, JsmConfig};
use klaviyo_connector::{klaviyo_spec, KlaviyoConfig};
use lever_connector::{lever_spec, LeverConfig};
use mailchimp_connector::{mailchimp_spec, MailchimpConfig};
use msgraph_connector::{msgraph_spec, MsGraphConfig};
use notion_connector::{notion_spec, NotionConfig};
use okta_connector::{okta_spec, OktaConfig};
use opsgenie_connector::{opsgenie_spec, OpsgenieConfig};
use pagerduty_connector::{pagerduty_spec, PagerDutyConfig};
use paypal_connector::{paypal_spec, PaypalConfig};
use pipedrive_connector::{pipedrive_spec, PipedriveConfig};
use recurly_connector::{recurly_spec, RecurlyConfig};
use rest_connector::{AuthSpec, ImportOptions, SourceSpec};
use sendgrid_connector::{sendgrid_spec, SendgridConfig};
use sentry_connector::{sentry_spec, SentryConfig};
use servicenow_connector::{servicenow_spec, ServiceNowConfig};
use shopify_connector::{shopify_spec, ShopifyConfig};
use slack_connector::{slack_spec, SlackConfig};
use smartsheet_connector::{smartsheet_spec, SmartsheetConfig};
use square_connector::{square_spec, SquareConfig};
use stripe_connector::stripe_spec;
use surveymonkey_connector::{surveymonkey_spec, SurveymonkeyConfig};
use twilio_connector::{twilio_spec, TwilioConfig};
use typeform_connector::{typeform_spec, TypeformConfig};
use woocommerce_connector::{woocommerce_spec, WooCommerceConfig};
use xero_connector::{xero_spec, XeroConfig};
use zendesk_connector::{zendesk_spec, ZendeskConfig};
use zohocrm_connector::{zohocrm_spec, ZohoCrmConfig};
use zoom_connector::{zoom_spec, ZoomConfig};

/// Something went wrong resolving a named connector.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("unknown connector '{name}' (known: {known})")]
    Unknown { name: String, known: String },
    #[error("connector '{connector}' requires the '{option}' option")]
    MissingOption { connector: String, option: String },
    #[error("could not build spec for '{connector}': {message}")]
    InvalidSpec { connector: String, message: String },
}

/// The built-in connector names.
pub fn list() -> &'static [&'static str] {
    &[
        "stripe",
        "github",
        "gitlab",
        "zendesk",
        "pagerduty",
        "freshdesk",
        "contentful",
        "asana",
        "shopify",
        "intercom",
        "pipedrive",
        "sentry",
        "hubspot",
        "slack",
        "mailchimp",
        "zoom",
        "servicenow",
        "okta",
        "auth0",
        "twilio",
        "typeform",
        "opsgenie",
        "smartsheet",
        "calendly",
        "bitbucket",
        "square",
        "recurly",
        "confluence",
        "woocommerce",
        "bigcommerce",
        "zohocrm",
        "activecampaign",
        "surveymonkey",
        "sendgrid",
        "greenhouse",
        "lever",
        "chargebee",
        "paypal",
        "docusign",
        "box",
        "jsm",
        "grafana",
        "klaviyo",
        "datadog",
        "xero",
        "msgraph",
        "gdrive",
        "gcalendar",
        "notion",
        "huggingface",
        "granola",
        "openapi",
    ]
}

/// Build a [`SourceSpec`] for the named connector from `options`.
pub fn spec_for(name: &str, options: &HashMap<String, String>) -> Result<SourceSpec, CatalogError> {
    let get = |k: &str| options.get(k).map(String::as_str);
    let require = |k: &str| {
        get(k).ok_or_else(|| CatalogError::MissingOption {
            connector: name.to_string(),
            option: k.to_string(),
        })
    };

    match name {
        "stripe" => Ok(stripe_spec(require("api_key")?, get("base_url"))),

        "github" => Ok(github_spec(&GithubConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.github.com")
                .to_string(),
            owner: require("owner")?.to_string(),
            repo: get("repo").unwrap_or_default().to_string(),
            token: get("token").map(str::to_string),
        })),

        "gitlab" => Ok(gitlab_spec(&GitLabConfig {
            base_url: get("base_url").unwrap_or("https://gitlab.com").to_string(),
            token: get("token").map(str::to_string),
        })),

        "zendesk" => Ok(zendesk_spec(&ZendeskConfig {
            base_url: require("base_url")?.to_string(),
            email: require("email")?.to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "pagerduty" => Ok(pagerduty_spec(&PagerDutyConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.pagerduty.com")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "freshdesk" => Ok(freshdesk_spec(&FreshdeskConfig {
            base_url: require("base_url")?.to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "contentful" => Ok(contentful_spec(&ContentfulConfig {
            base_url: require("base_url")?.to_string(),
            access_token: require("access_token")?.to_string(),
        })),

        "asana" => Ok(asana_spec(&AsanaConfig {
            base_url: get("base_url")
                .unwrap_or("https://app.asana.com/api/1.0")
                .to_string(),
            token: get("token").map(str::to_string),
        })),

        "shopify" => Ok(shopify_spec(&ShopifyConfig {
            base_url: require("base_url")?.to_string(),
            access_token: require("access_token")?.to_string(),
        })),

        "intercom" => Ok(intercom_spec(&IntercomConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.intercom.io")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "pipedrive" => Ok(pipedrive_spec(&PipedriveConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.pipedrive.com/v1")
                .to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "sentry" => Ok(sentry_spec(&SentryConfig {
            base_url: get("base_url")
                .unwrap_or("https://sentry.io/api/0")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "hubspot" => Ok(hubspot_spec(&HubspotConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.hubapi.com")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "slack" => Ok(slack_spec(&SlackConfig {
            base_url: get("base_url")
                .unwrap_or("https://slack.com/api")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "mailchimp" => Ok(mailchimp_spec(&MailchimpConfig {
            base_url: require("base_url")?.to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "zoom" => Ok(zoom_spec(&ZoomConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.zoom.us/v2")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "servicenow" => Ok(servicenow_spec(&ServiceNowConfig {
            base_url: require("base_url")?.to_string(),
            username: require("username")?.to_string(),
            password: require("password")?.to_string(),
        })),

        "okta" => Ok(okta_spec(&OktaConfig {
            base_url: require("base_url")?.to_string(),
            token: require("token")?.to_string(),
        })),

        "auth0" => Ok(auth0_spec(&Auth0Config {
            base_url: require("base_url")?.to_string(),
            token: require("token")?.to_string(),
        })),

        "twilio" => Ok(twilio_spec(&TwilioConfig {
            base_url: require("base_url")?.to_string(),
            account_sid: require("account_sid")?.to_string(),
            auth_token: require("auth_token")?.to_string(),
        })),

        "typeform" => Ok(typeform_spec(&TypeformConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.typeform.com")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "opsgenie" => Ok(opsgenie_spec(&OpsgenieConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.opsgenie.com/v2")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "smartsheet" => Ok(smartsheet_spec(&SmartsheetConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.smartsheet.com/2.0")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "calendly" => Ok(calendly_spec(&CalendlyConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.calendly.com")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "bitbucket" => Ok(bitbucket_spec(&BitbucketConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.bitbucket.org/2.0")
                .to_string(),
            username: require("username")?.to_string(),
            app_password: require("app_password")?.to_string(),
        })),

        "square" => Ok(square_spec(&SquareConfig {
            base_url: get("base_url")
                .unwrap_or("https://connect.squareup.com/v2")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "recurly" => Ok(recurly_spec(&RecurlyConfig {
            base_url: get("base_url")
                .unwrap_or("https://v3.recurly.com")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "confluence" => Ok(confluence_spec(&ConfluenceConfig {
            base_url: require("base_url")?.to_string(),
            email: require("email")?.to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "woocommerce" => Ok(woocommerce_spec(&WooCommerceConfig {
            base_url: require("base_url")?.to_string(),
            consumer_key: require("consumer_key")?.to_string(),
            consumer_secret: require("consumer_secret")?.to_string(),
        })),

        "bigcommerce" => Ok(bigcommerce_spec(&BigCommerceConfig {
            base_url: require("base_url")?.to_string(),
            access_token: require("access_token")?.to_string(),
        })),

        "zohocrm" => Ok(zohocrm_spec(&ZohoCrmConfig {
            base_url: get("base_url")
                .unwrap_or("https://www.zohoapis.com/crm/v3")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "activecampaign" => Ok(activecampaign_spec(&ActiveCampaignConfig {
            base_url: require("base_url")?.to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "surveymonkey" => Ok(surveymonkey_spec(&SurveymonkeyConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.surveymonkey.com/v3")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "sendgrid" => Ok(sendgrid_spec(&SendgridConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.sendgrid.com/v3")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "greenhouse" => Ok(greenhouse_spec(&GreenhouseConfig {
            base_url: get("base_url")
                .unwrap_or("https://harvest.greenhouse.io/v1")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "lever" => Ok(lever_spec(&LeverConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.lever.co/v1")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "chargebee" => Ok(chargebee_spec(&ChargebeeConfig {
            base_url: require("base_url")?.to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "paypal" => Ok(paypal_spec(&PaypalConfig {
            base_url: get("base_url")
                .unwrap_or("https://api-m.paypal.com")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "docusign" => Ok(docusign_spec(&DocusignConfig {
            base_url: require("base_url")?.to_string(),
            token: require("token")?.to_string(),
        })),

        "box" => Ok(box_spec(&BoxConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.box.com/2.0")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "jsm" => Ok(jsm_spec(&JsmConfig {
            base_url: require("base_url")?.to_string(),
            email: require("email")?.to_string(),
            api_token: require("api_token")?.to_string(),
        })),

        "grafana" => Ok(grafana_spec(&GrafanaConfig {
            base_url: require("base_url")?.to_string(),
            token: require("token")?.to_string(),
        })),

        "klaviyo" => Ok(klaviyo_spec(&KlaviyoConfig {
            base_url: get("base_url")
                .unwrap_or("https://a.klaviyo.com/api")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        "datadog" => Ok(datadog_spec(&DatadogConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.datadoghq.com/api")
                .to_string(),
            api_key: require("api_key")?.to_string(),
            app_key: require("app_key")?.to_string(),
        })),

        "xero" => Ok(xero_spec(&XeroConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.xero.com/api.xro/2.0")
                .to_string(),
            token: require("token")?.to_string(),
            tenant_id: require("tenant_id")?.to_string(),
        })),

        "msgraph" => Ok(msgraph_spec(&MsGraphConfig {
            base_url: get("base_url")
                .unwrap_or("https://graph.microsoft.com/v1.0")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "gdrive" => Ok(gdrive_spec(&GdriveConfig {
            base_url: get("base_url")
                .unwrap_or("https://www.googleapis.com/drive/v3")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "gcalendar" => Ok(gcalendar_spec(&GcalendarConfig {
            base_url: get("base_url")
                .unwrap_or("https://www.googleapis.com/calendar/v3")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "notion" => Ok(notion_spec(&NotionConfig {
            base_url: get("base_url")
                .unwrap_or("https://api.notion.com/v1")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "huggingface" => Ok(huggingface_spec(&HuggingFaceConfig {
            base_url: get("base_url")
                .unwrap_or("https://huggingface.co")
                .to_string(),
            token: require("token")?.to_string(),
        })),

        "granola" => Ok(granola_spec(&GranolaConfig {
            base_url: get("base_url")
                .unwrap_or("https://public-api.granola.ai/v1")
                .to_string(),
            api_key: require("api_key")?.to_string(),
        })),

        // Bring-your-own API: generate a spec from an OpenAPI document.
        "openapi" => {
            let doc = require("spec")?;
            let auth = match get("token").or_else(|| get("api_key")) {
                Some(t) => AuthSpec::Bearer {
                    token: t.to_string(),
                },
                None => AuthSpec::None,
            };
            let opts = ImportOptions {
                auth,
                base_url: get("base_url").map(str::to_string),
                ..Default::default()
            };
            SourceSpec::from_openapi_json(doc, opts).map_err(|e| CatalogError::InvalidSpec {
                connector: name.to_string(),
                message: e.to_string(),
            })
        }

        other => Err(CatalogError::Unknown {
            name: other.to_string(),
            known: list().join(", "),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn stripe_needs_only_an_api_key() {
        let spec = spec_for("stripe", &opts(&[("api_key", "sk_test_x")])).unwrap();
        assert_eq!(spec.name, "stripe");
        assert!(spec.table("charges").is_some());
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
    }

    #[test]
    fn github_needs_an_owner() {
        let spec = spec_for(
            "github",
            &opts(&[("owner", "octocat"), ("repo", "Hello-World")]),
        )
        .unwrap();
        assert_eq!(spec.name, "github");
        assert!(spec.table("repos").is_some());
    }

    #[test]
    fn built_in_connectors_resolve_from_options() {
        // GitLab and PagerDuty default their base_url; the rest require it.
        let gl = spec_for("gitlab", &opts(&[("token", "t")])).unwrap();
        assert_eq!(gl.name, "gitlab");
        assert!(gl.table("projects").is_some());

        let zd = spec_for(
            "zendesk",
            &opts(&[
                ("base_url", "https://acme.zendesk.com"),
                ("email", "a@b.c"),
                ("api_token", "t"),
            ]),
        )
        .unwrap();
        assert!(zd.table("tickets").is_some());

        let pd = spec_for("pagerduty", &opts(&[("api_key", "k")])).unwrap();
        assert!(pd.table("incidents").is_some());

        let fd = spec_for(
            "freshdesk",
            &opts(&[("base_url", "https://acme.freshdesk.com"), ("api_key", "k")]),
        )
        .unwrap();
        assert!(fd.table("tickets").is_some());

        let cf = spec_for(
            "contentful",
            &opts(&[
                (
                    "base_url",
                    "https://cdn.contentful.com/spaces/s/environments/master",
                ),
                ("access_token", "t"),
            ]),
        )
        .unwrap();
        assert!(cf.table("entries").is_some());

        // A required option is enforced for these too.
        assert!(matches!(
            spec_for("zendesk", &opts(&[("email", "a@b.c")])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
    }

    #[test]
    fn batch1_connectors_resolve_from_options() {
        // Asana defaults its base_url and token is optional.
        let asana = spec_for("asana", &opts(&[])).unwrap();
        assert_eq!(asana.name, "asana");
        assert!(asana.table("projects").is_some());

        let shopify = spec_for(
            "shopify",
            &opts(&[
                ("base_url", "https://acme.myshopify.com/admin/api/2024-01"),
                ("access_token", "t"),
            ]),
        )
        .unwrap();
        assert!(shopify.table("products").is_some());

        let intercom = spec_for("intercom", &opts(&[("token", "t")])).unwrap();
        assert!(intercom.table("contacts").is_some());

        let pipedrive = spec_for("pipedrive", &opts(&[("api_token", "t")])).unwrap();
        assert!(pipedrive.table("deals").is_some());

        let sentry = spec_for("sentry", &opts(&[("token", "t")])).unwrap();
        assert!(sentry.table("projects").is_some());

        // Required options are enforced.
        assert!(matches!(
            spec_for("shopify", &opts(&[("access_token", "t")])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("intercom", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("pipedrive", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("sentry", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
    }

    #[test]
    fn batch2_connectors_resolve_from_options() {
        let hs = spec_for("hubspot", &opts(&[("token", "t")])).unwrap();
        assert_eq!(hs.name, "hubspot");
        assert!(hs.table("contacts").is_some());

        let sl = spec_for("slack", &opts(&[("token", "t")])).unwrap();
        assert!(sl.table("users").is_some());

        let mc = spec_for(
            "mailchimp",
            &opts(&[
                ("base_url", "https://us1.api.mailchimp.com/3.0"),
                ("api_key", "k"),
            ]),
        )
        .unwrap();
        assert!(mc.table("lists").is_some());

        let zm = spec_for("zoom", &opts(&[("token", "t")])).unwrap();
        assert!(zm.table("users").is_some());

        let sn = spec_for(
            "servicenow",
            &opts(&[
                ("base_url", "https://dev.service-now.com/api/now"),
                ("username", "u"),
                ("password", "p"),
            ]),
        )
        .unwrap();
        assert!(sn.table("incident").is_some());

        // Required options enforced.
        for (name, o) in [
            ("hubspot", opts(&[])),
            ("slack", opts(&[])),
            ("mailchimp", opts(&[("api_key", "k")])),
            ("zoom", opts(&[])),
            ("servicenow", opts(&[("username", "u"), ("password", "p")])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch3_connectors_resolve_from_options() {
        assert!(spec_for(
            "okta",
            &opts(&[("base_url", "https://x.okta.com/api/v1"), ("token", "t")])
        )
        .unwrap()
        .table("users")
        .is_some());
        assert!(spec_for(
            "auth0",
            &opts(&[("base_url", "https://x.auth0.com/api/v2"), ("token", "t")])
        )
        .unwrap()
        .table("users")
        .is_some());
        assert!(spec_for(
            "twilio",
            &opts(&[
                ("base_url", "https://api.twilio.com/x"),
                ("account_sid", "AC"),
                ("auth_token", "t")
            ])
        )
        .unwrap()
        .table("messages")
        .is_some());
        assert!(spec_for("typeform", &opts(&[("token", "t")]))
            .unwrap()
            .table("forms")
            .is_some());
        assert!(spec_for("opsgenie", &opts(&[("api_key", "k")]))
            .unwrap()
            .table("alerts")
            .is_some());
        for (name, o) in [
            ("okta", opts(&[("token", "t")])),
            ("auth0", opts(&[("token", "t")])),
            (
                "twilio",
                opts(&[("base_url", "https://x"), ("account_sid", "AC")]),
            ),
            ("typeform", opts(&[])),
            ("opsgenie", opts(&[])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch4_connectors_resolve_from_options() {
        assert!(spec_for("smartsheet", &opts(&[("token", "t")]))
            .unwrap()
            .table("sheets")
            .is_some());
        assert!(spec_for("calendly", &opts(&[("token", "t")]))
            .unwrap()
            .table("event_types")
            .is_some());
        assert!(spec_for(
            "bitbucket",
            &opts(&[("username", "u"), ("app_password", "p")])
        )
        .unwrap()
        .table("repositories")
        .is_some());
        assert!(spec_for("square", &opts(&[("token", "t")]))
            .unwrap()
            .table("customers")
            .is_some());
        assert!(spec_for("recurly", &opts(&[("api_key", "k")]))
            .unwrap()
            .table("accounts")
            .is_some());
        for (name, o) in [
            ("smartsheet", opts(&[])),
            ("calendly", opts(&[])),
            ("bitbucket", opts(&[("username", "u")])),
            ("square", opts(&[])),
            ("recurly", opts(&[])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch5_connectors_resolve_from_options() {
        assert!(spec_for(
            "confluence",
            &opts(&[
                ("base_url", "https://x.atlassian.net/wiki/rest/api"),
                ("email", "a@b.c"),
                ("api_token", "t")
            ])
        )
        .unwrap()
        .table("content")
        .is_some());
        assert!(spec_for(
            "woocommerce",
            &opts(&[
                ("base_url", "https://shop/wp-json/wc/v3"),
                ("consumer_key", "ck"),
                ("consumer_secret", "cs")
            ])
        )
        .unwrap()
        .table("products")
        .is_some());
        assert!(spec_for(
            "bigcommerce",
            &opts(&[
                ("base_url", "https://api.bigcommerce.com/stores/x/v3"),
                ("access_token", "t")
            ])
        )
        .unwrap()
        .table("products")
        .is_some());
        assert!(spec_for("zohocrm", &opts(&[("token", "t")]))
            .unwrap()
            .table("Leads")
            .is_some());
        assert!(spec_for(
            "activecampaign",
            &opts(&[
                ("base_url", "https://x.api-us1.com/api/3"),
                ("api_token", "t")
            ])
        )
        .unwrap()
        .table("contacts")
        .is_some());
        for (name, o) in [
            (
                "confluence",
                opts(&[("email", "a@b.c"), ("api_token", "t")]),
            ),
            ("woocommerce", opts(&[("consumer_key", "ck")])),
            ("bigcommerce", opts(&[])),
            ("zohocrm", opts(&[])),
            ("activecampaign", opts(&[])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch6_connectors_resolve_from_options() {
        assert!(spec_for("surveymonkey", &opts(&[("token", "t")]))
            .unwrap()
            .table("surveys")
            .is_some());
        assert!(spec_for("sendgrid", &opts(&[("api_key", "k")]))
            .unwrap()
            .table("templates")
            .is_some());
        assert!(spec_for("greenhouse", &opts(&[("api_key", "k")]))
            .unwrap()
            .table("candidates")
            .is_some());
        assert!(spec_for("lever", &opts(&[("api_key", "k")]))
            .unwrap()
            .table("opportunities")
            .is_some());
        assert!(spec_for(
            "chargebee",
            &opts(&[
                ("base_url", "https://x.chargebee.com/api/v2"),
                ("api_key", "k")
            ])
        )
        .unwrap()
        .table("subscriptions")
        .is_some());
        for (name, o) in [
            ("surveymonkey", opts(&[])),
            ("sendgrid", opts(&[])),
            ("greenhouse", opts(&[])),
            ("lever", opts(&[])),
            ("chargebee", opts(&[("api_key", "k")])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch7_connectors_resolve_from_options() {
        assert!(spec_for("paypal", &opts(&[("token", "t")]))
            .unwrap()
            .table("invoices")
            .is_some());
        assert!(spec_for(
            "docusign",
            &opts(&[
                ("base_url", "https://x/restapi/v2.1/accounts/1"),
                ("token", "t")
            ])
        )
        .unwrap()
        .table("templates")
        .is_some());
        assert!(spec_for("box", &opts(&[("token", "t")]))
            .unwrap()
            .table("users")
            .is_some());
        assert!(spec_for(
            "jsm",
            &opts(&[
                ("base_url", "https://x.atlassian.net/rest/servicedeskapi"),
                ("email", "a@b.c"),
                ("api_token", "t")
            ])
        )
        .unwrap()
        .table("request")
        .is_some());
        assert!(spec_for(
            "grafana",
            &opts(&[("base_url", "https://g/api"), ("token", "t")])
        )
        .unwrap()
        .table("datasources")
        .is_some());
        for (name, o) in [
            ("paypal", opts(&[])),
            ("docusign", opts(&[("token", "t")])),
            ("box", opts(&[])),
            ("jsm", opts(&[("email", "a@b.c")])),
            ("grafana", opts(&[("token", "t")])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch8_connectors_resolve_from_options() {
        assert!(spec_for("klaviyo", &opts(&[("api_key", "k")]))
            .unwrap()
            .table("profiles")
            .is_some());
        assert!(
            spec_for("datadog", &opts(&[("api_key", "k"), ("app_key", "a")]))
                .unwrap()
                .table("monitors")
                .is_some()
        );
        assert!(
            spec_for("xero", &opts(&[("token", "t"), ("tenant_id", "x")]))
                .unwrap()
                .table("Invoices")
                .is_some()
        );
        assert!(spec_for("msgraph", &opts(&[("token", "t")]))
            .unwrap()
            .table("users")
            .is_some());
        assert!(spec_for("gdrive", &opts(&[("token", "t")]))
            .unwrap()
            .table("files")
            .is_some());
        for (name, o) in [
            ("klaviyo", opts(&[])),
            ("datadog", opts(&[("api_key", "k")])),
            ("xero", opts(&[("token", "t")])),
            ("msgraph", opts(&[])),
            ("gdrive", opts(&[])),
        ] {
            assert!(matches!(
                spec_for(name, &o).unwrap_err(),
                CatalogError::MissingOption { .. }
            ));
        }
    }

    #[test]
    fn batch9_connectors_resolve_from_options() {
        assert!(spec_for("gcalendar", &opts(&[("token", "t")]))
            .unwrap()
            .table("calendars")
            .is_some());
        assert!(spec_for("notion", &opts(&[("token", "t")]))
            .unwrap()
            .table("users")
            .is_some());
        assert!(matches!(
            spec_for("gcalendar", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
        assert!(matches!(
            spec_for("notion", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
    }

    #[test]
    fn granola_resolves() {
        let spec = spec_for("granola", &opts(&[("api_key", "grn_x")])).unwrap();
        assert_eq!(spec.name, "granola");
        assert!(spec.table("notes").is_some());
        assert!(matches!(
            spec_for("granola", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
    }

    #[test]
    fn huggingface_resolves() {
        let spec = spec_for("huggingface", &opts(&[("token", "hf_x")])).unwrap();
        assert_eq!(spec.name, "huggingface");
        assert!(spec.table("models").is_some());
        assert!(matches!(
            spec_for("huggingface", &opts(&[])).unwrap_err(),
            CatalogError::MissingOption { .. }
        ));
    }

    #[test]
    fn openapi_imports_a_document_with_optional_auth() {
        let doc = serde_json::json!({
            "servers": [{"url": "https://x"}],
            "paths": {"/t": {"get": {"responses": {"200": {"content":
                {"application/json": {"schema": {"type": "array", "items":
                    {"type": "object", "properties": {"id": {"type": "string"}}}}}}}}}}}
        })
        .to_string();
        let spec = spec_for("openapi", &opts(&[("spec", doc.as_str()), ("token", "t")])).unwrap();
        assert!(spec.table("t").is_some());
        assert!(matches!(spec.auth, AuthSpec::Bearer { .. }));
        // Without a token, auth defaults to none.
        let spec2 = spec_for("openapi", &opts(&[("spec", doc.as_str())])).unwrap();
        assert!(matches!(spec2.auth, AuthSpec::None));
    }

    #[test]
    fn missing_required_option_errors() {
        let err = spec_for("stripe", &opts(&[])).unwrap_err();
        assert!(matches!(err, CatalogError::MissingOption { .. }));
        assert!(err.to_string().contains("api_key"));
    }

    #[test]
    fn invalid_openapi_errors() {
        let err = spec_for("openapi", &opts(&[("spec", "not json")])).unwrap_err();
        assert!(matches!(err, CatalogError::InvalidSpec { .. }));
    }

    #[test]
    fn unknown_connector_errors_and_lists_known() {
        let err = spec_for("salesforce", &opts(&[])).unwrap_err();
        assert!(matches!(err, CatalogError::Unknown { .. }));
        assert!(err.to_string().contains("stripe"));
        assert!(!list().is_empty());
    }
}
