// Copyright 2025 Bloxide, all rights reserved
use crate::model::BloxSpec;
use crate::parser::parse_spec;

pub fn parse_json_spec(name: &str, json: &str) -> Result<BloxSpec, String> {
    let mut spec: BloxSpec =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse JSON: {}", e))?;
    // Override name from parameter in case the JSON has a different one
    spec.name = name.to_string();
    Ok(spec)
}

pub fn load_specs() -> Vec<BloxSpec> {
    let counter_md = include_str!("../../../spec/bloxes/counter.md");
    let ping_md = include_str!("../../../spec/bloxes/ping.md");
    let pong_md = include_str!("../../../spec/bloxes/pong.md");
    let pool_md = include_str!("../../../spec/bloxes/pool.md");
    let worker_md = include_str!("../../../spec/bloxes/worker.md");

    vec![
        parse_spec("Counter", counter_md),
        parse_spec("Ping", ping_md),
        parse_spec("Pong", pong_md),
        parse_spec("Pool", pool_md),
        parse_spec("Worker", worker_md),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_counter() {
        let md = include_str!("../../../spec/bloxes/counter.md");
        let spec = parse_spec("Counter", md);

        println!("States: {:?}", spec.states);
        println!("Events: {:?}", spec.events);
        println!("Handlers: {:?}", spec.handlers);
        println!("EntryExit: {:?}", spec.entry_exit);

        // Should have at least Ready and Done states
        assert!(spec.states.iter().any(|s| s.name == "Ready"));
        assert!(spec.states.iter().any(|s| s.name == "Done"));

        // Should have CounterMsg::Tick event
        assert!(spec
            .events
            .iter()
            .any(|e| e.full_name == "CounterMsg::Tick"));

        // Ready should have a handler for Tick
        assert!(spec
            .handlers
            .iter()
            .any(|h| h.state == "Ready" && h.event == "CounterMsg::Tick"));
    }

    #[test]
    fn test_parse_ping() {
        let md = include_str!("../../../spec/bloxes/ping.md");
        let spec = parse_spec("Ping", md);

        println!("States: {:?}", spec.states);
        println!("Events: {:?}", spec.events);
        println!("Handlers: {:?}", spec.handlers);

        // Should have key states
        assert!(spec.states.iter().any(|s| s.name == "Operating"));
        assert!(spec.states.iter().any(|s| s.name == "Active"));
        assert!(spec.states.iter().any(|s| s.name == "Paused"));
        assert!(spec.states.iter().any(|s| s.name == "Done"));
        assert!(spec.states.iter().any(|s| s.name == "Error"));

        // Should have PingPongMsg events
        assert!(spec
            .events
            .iter()
            .any(|e| e.full_name == "PingPongMsg::Pong"));
        assert!(spec
            .events
            .iter()
            .any(|e| e.full_name == "PingPongMsg::Resume"));

        // Active should have handler for Pong
        assert!(spec
            .handlers
            .iter()
            .any(|h| h.state == "Active" && h.event == "PingPongMsg::Pong"));
    }

    #[test]
    fn test_parse_json_counter() {
        // This test verifies that JSON exported by bloxide-viz-export can be loaded
        // Run `cargo run` in ../bloxide-viz-export to generate the fixture first.
        let json = std::fs::read_to_string("../bloxide-viz-export/bloxide-viz-output/counter.json");
        if let Ok(json) = json {
            let spec = parse_json_spec("Counter", &json).unwrap();
            assert!(spec.states.iter().any(|s| s.name == "Ready"));
            assert!(spec.states.iter().any(|s| s.name == "Done"));
            // Should have at least one handler extracted from transitions!
            assert!(!spec.handlers.is_empty());
        }
    }
}
