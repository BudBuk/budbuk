//! Per-connector option metadata.
//!
//! [`spec_for`](crate::spec_for) encodes which options each connector needs
//! imperatively (`require` vs `get`); this module exposes the same knowledge
//! declaratively so a UI can render the right fields and mask secrets.

/// One configuration option a connector accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionSpec {
    /// The option key, e.g. `"api_key"`.
    pub key: &'static str,
    /// Whether the connector requires it.
    pub required: bool,
    /// Whether it's a credential/secret (should be masked and encrypted).
    pub secret: bool,
}

/// Credential-like keys that must be masked in a UI and encrypted at rest.
fn is_secret(key: &str) -> bool {
    matches!(
        key,
        "api_key"
            | "api_token"
            | "token"
            | "access_token"
            | "password"
            | "app_password"
            | "auth_token"
            | "app_key"
            | "consumer_secret"
    )
}

fn spec(key: &'static str, required: bool) -> OptionSpec {
    OptionSpec {
        key,
        required,
        secret: is_secret(key),
    }
}

/// The options a named connector accepts (required first, `base_url` last).
/// Unknown connectors return an empty list.
pub fn options_for(name: &str) -> Vec<OptionSpec> {
    match name {
        "stripe" => vec![spec("api_key", true), spec("base_url", false)],
        "github" => vec![
            spec("owner", true),
            spec("repo", false),
            spec("token", false),
            spec("base_url", false),
        ],
        "gitlab" => vec![spec("token", false), spec("base_url", false)],
        "zendesk" => vec![
            spec("base_url", true),
            spec("email", true),
            spec("api_token", true),
        ],
        "pagerduty" => vec![spec("api_key", true), spec("base_url", false)],
        "freshdesk" => vec![spec("base_url", true), spec("api_key", true)],
        "contentful" => vec![spec("base_url", true), spec("access_token", true)],
        "asana" => vec![spec("token", false), spec("base_url", false)],
        "shopify" => vec![spec("base_url", true), spec("access_token", true)],
        "intercom" => vec![spec("token", true), spec("base_url", false)],
        "pipedrive" => vec![spec("api_token", true), spec("base_url", false)],
        "sentry" => vec![spec("token", true), spec("base_url", false)],
        "hubspot" => vec![spec("token", true), spec("base_url", false)],
        "slack" => vec![spec("token", true), spec("base_url", false)],
        "mailchimp" => vec![spec("base_url", true), spec("api_key", true)],
        "zoom" => vec![spec("token", true), spec("base_url", false)],
        "servicenow" => vec![
            spec("base_url", true),
            spec("username", true),
            spec("password", true),
        ],
        "okta" => vec![spec("base_url", true), spec("token", true)],
        "auth0" => vec![spec("base_url", true), spec("token", true)],
        "twilio" => vec![
            spec("base_url", true),
            spec("account_sid", true),
            spec("auth_token", true),
        ],
        "typeform" => vec![spec("token", true), spec("base_url", false)],
        "opsgenie" => vec![spec("api_key", true), spec("base_url", false)],
        "smartsheet" => vec![spec("token", true), spec("base_url", false)],
        "calendly" => vec![spec("token", true), spec("base_url", false)],
        "bitbucket" => vec![
            spec("username", true),
            spec("app_password", true),
            spec("base_url", false),
        ],
        "square" => vec![spec("token", true), spec("base_url", false)],
        "recurly" => vec![spec("api_key", true), spec("base_url", false)],
        "confluence" => vec![
            spec("base_url", true),
            spec("email", true),
            spec("api_token", true),
        ],
        "woocommerce" => vec![
            spec("base_url", true),
            spec("consumer_key", true),
            spec("consumer_secret", true),
        ],
        "bigcommerce" => vec![spec("base_url", true), spec("access_token", true)],
        "zohocrm" => vec![spec("token", true), spec("base_url", false)],
        "activecampaign" => vec![spec("base_url", true), spec("api_token", true)],
        "surveymonkey" => vec![spec("token", true), spec("base_url", false)],
        "sendgrid" => vec![spec("api_key", true), spec("base_url", false)],
        "greenhouse" => vec![spec("api_key", true), spec("base_url", false)],
        "lever" => vec![spec("api_key", true), spec("base_url", false)],
        "chargebee" => vec![spec("base_url", true), spec("api_key", true)],
        "paypal" => vec![spec("token", true), spec("base_url", false)],
        "docusign" => vec![spec("base_url", true), spec("token", true)],
        "box" => vec![spec("token", true), spec("base_url", false)],
        "jsm" => vec![
            spec("base_url", true),
            spec("email", true),
            spec("api_token", true),
        ],
        "grafana" => vec![spec("base_url", true), spec("token", true)],
        "klaviyo" => vec![spec("api_key", true), spec("base_url", false)],
        "datadog" => vec![
            spec("api_key", true),
            spec("app_key", true),
            spec("base_url", false),
        ],
        "xero" => vec![
            spec("token", true),
            spec("tenant_id", true),
            spec("base_url", false),
        ],
        "msgraph" => vec![spec("token", true), spec("base_url", false)],
        "gdrive" => vec![spec("token", true), spec("base_url", false)],
        "gcalendar" => vec![spec("token", true), spec("base_url", false)],
        "notion" => vec![spec("token", true), spec("base_url", false)],
        "huggingface" => vec![spec("token", true), spec("base_url", false)],
        "granola" => vec![spec("api_key", true), spec("base_url", false)],
        "openapi" => vec![
            spec("spec", true),
            spec("token", false),
            spec("api_key", false),
            spec("base_url", false),
        ],
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::list;

    #[test]
    fn every_listed_connector_has_options() {
        for name in list() {
            let opts = options_for(name);
            assert!(!opts.is_empty(), "no options for {name}");
        }
        // Most connectors need at least one required option; gitlab and asana are
        // the exceptions (they work against public data with no credentials).
        assert!(options_for("slack").iter().any(|o| o.required));
        assert!(!options_for("gitlab").iter().any(|o| o.required));
    }

    #[test]
    fn unknown_connector_has_no_options() {
        assert!(options_for("does-not-exist").is_empty());
    }

    #[test]
    fn secrets_and_requireds_are_flagged() {
        let slack = options_for("slack");
        let token = slack.iter().find(|o| o.key == "token").unwrap();
        assert!(token.required && token.secret);
        let base = slack.iter().find(|o| o.key == "base_url").unwrap();
        assert!(!base.required && !base.secret);

        // owner is required but not a secret; every secret-named key is secret.
        let gh = options_for("github");
        assert!(gh.iter().find(|o| o.key == "owner").unwrap().required);
        assert!(!gh.iter().find(|o| o.key == "owner").unwrap().secret);

        // Non-credential identifiers are not secret.
        for key in [
            "email",
            "username",
            "account_sid",
            "consumer_key",
            "tenant_id",
            "spec",
        ] {
            assert!(!is_secret(key), "{key} wrongly flagged secret");
        }
    }
}
