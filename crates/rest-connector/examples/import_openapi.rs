//! Import an OpenAPI document from a file and summarize the generated spec.
//! Run: cargo run -p rest-connector --example import_openapi -- <spec.json> [name-filter]

use rest_connector::{ImportOptions, SourceSpec};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: import_openapi <spec.json> [filter]");
    let filter = std::env::args().nth(2);
    let json = std::fs::read_to_string(&path).expect("read spec");

    match SourceSpec::from_openapi_json(&json, ImportOptions::default()) {
        Ok(spec) => {
            let tables: Vec<_> = spec
                .tables
                .iter()
                .filter(|t| filter.as_deref().is_none_or(|f| t.name.contains(f)))
                .collect();
            println!(
                "source '{}' — base_url={} — {} tables ({} shown)",
                spec.name,
                spec.base_url,
                spec.tables.len(),
                tables.len()
            );
            for t in tables {
                println!(
                    "  {:<28} {:<40} cols={:<3} filters={:<2} {:?}",
                    t.name,
                    t.path,
                    t.columns.len(),
                    t.filters.len(),
                    t.pagination
                );
            }
        }
        Err(e) => eprintln!("import failed: {e}"),
    }
}
