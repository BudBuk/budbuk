//! Import an OpenAPI document and print the resulting SourceSpec as JSON, ready
//! to paste into a rest-fdw `spec` server option.
//!
//! Usage: spec_json <openapi.json> [base_url_override] [include,comma,list]
//! A Bearer token can be supplied via the BUDBUK_BEARER env var.

use rest_connector::{AuthSpec, ImportOptions, SourceSpec};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .expect("usage: spec_json <openapi.json> [base_url] [include]");
    let base_url = args.get(2).filter(|s| !s.is_empty()).cloned();
    let include = args
        .get(3)
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(|x| x.trim().to_string()).collect());
    let auth = match std::env::var("BUDBUK_BEARER") {
        Ok(token) if !token.is_empty() => AuthSpec::Bearer { token },
        _ => AuthSpec::None,
    };

    let json = std::fs::read_to_string(path).expect("read spec");
    let opts = ImportOptions {
        base_url,
        include,
        auth,
        ..Default::default()
    };
    let spec = SourceSpec::from_openapi_json(&json, opts).expect("import");
    println!("{}", serde_json::to_string(&spec).unwrap());
}
