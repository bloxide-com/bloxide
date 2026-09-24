// Copyright 2025 Bloxide, all rights reserved
use crate::model::BloxSpec;

/// Parse a BloxSpec from viz-export JSON (the single data format — blox.toml
/// is the sole source, exported via bloxide-viz-export; issue #119).
pub fn parse_json_spec(name: &str, json: &str) -> Result<BloxSpec, String> {
    let mut spec: BloxSpec =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    // Override name from parameter in case the JSON has a different one
    spec.name = name.to_string();
    Ok(spec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_counter() {
        // Exercise the real exporter and current TOML source. A missing fixture
        // must fail the test rather than silently skipping all assertions.
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let specs = bloxide_viz_export::export_workspace(&workspace).unwrap();
        let counter = specs.iter().find(|spec| spec.name == "Counter").unwrap();
        let json = serde_json::to_string(counter).unwrap();
        let spec = parse_json_spec("ImportedCounter", &json).unwrap();
        assert_eq!(spec.name, "ImportedCounter");
        assert!(spec.states.iter().any(|s| s.name == "Ready"));
        assert!(!spec.handlers.is_empty());
    }
}
