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
        // This test verifies that JSON exported by bloxide-viz-export can be loaded
        // Run `cargo run` in ../bloxide-viz-export to generate the fixture first.
        let json = std::fs::read_to_string("../bloxide-viz-export/bloxide-viz-output/counter.json");
        if let Ok(json) = json {
            let spec = parse_json_spec("Counter", &json).unwrap();
            assert!(spec.states.iter().any(|s| s.name == "Ready"));
            // Should have at least one handler extracted from the transition rules
            assert!(!spec.handlers.is_empty());
        }
    }
}
