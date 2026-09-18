//! `--json` must emit exactly one self-describing document per invocation.
//!
//! Before this was pinned, `keil map` and `keil locate` printed two objects one
//! after another (so `| jq .` failed outright), and `ioc get` chose its shape
//! from the data rather than from the query.

mod common;

use common::{emb, write_fixture, write_map_fixture, write_scratch};
use serde_json::Value;

fn json_of(args: &[&str]) -> Value {
    let out = emb(args);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{stdout}"))
}

#[test]
fn keil_map_emits_a_single_document() {
    let map = write_map_fixture("shape.map");
    let v = json_of(&["--json", "keil", "map", map.to_str().unwrap()]);

    assert_eq!(v["kind"], "sections");
    let sections = v["sections"].as_array().expect("sections array");
    assert_eq!(sections.len(), 2, "{v}");
    assert_eq!(sections[0]["kind"], "kv");
    assert_eq!(sections[1]["kind"], "table");
}

#[test]
fn keil_locate_emits_a_single_document() {
    let proj = write_fixture("locate_shape.uvprojx");
    let v = json_of(&[
        "--json",
        "keil",
        "locate",
        proj.to_str().unwrap(),
        "-t",
        "OnlyTarget",
        "defines",
    ]);

    assert_eq!(v["kind"], "sections");
    assert_eq!(v["sections"].as_array().unwrap().len(), 2, "{v}");
}

#[test]
fn ioc_get_announces_its_shape() {
    let ioc = write_scratch(
        "shape.ioc",
        "Mcu.Name=STM32H7R7L8Hx\nGPIO.groupedBy=Show All\n",
    );
    let ioc = ioc.to_str().unwrap();

    let exact = json_of(&["--json", "ioc", "get", ioc, "Mcu.Name"]);
    assert_eq!(exact["kind"], "message");
    assert_eq!(exact["value"], "STM32H7R7L8Hx");

    let prefix = json_of(&["--json", "ioc", "get", ioc, "GPIO"]);
    assert_eq!(prefix["kind"], "kv");
    assert_eq!(prefix["entries"][0][0], "GPIO.groupedBy");
}

#[test]
fn tables_keep_headers_and_rows_at_the_top_level() {
    let proj = write_fixture("table_shape.uvprojx");
    let v = json_of(&["--json", "keil", "info", proj.to_str().unwrap()]);

    assert_eq!(v["kind"], "table");
    assert!(v["headers"].is_array(), "{v}");
    assert!(v["rows"].is_array(), "{v}");
}
