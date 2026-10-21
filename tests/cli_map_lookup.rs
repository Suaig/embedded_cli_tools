//! `keil map --lookup` turns a fault address into a symbol.
//!
//! A HardFault hands back a PC and an LR and nothing else; before this existed
//! the map had to be parsed by hand to find out what they were.

mod common;

use common::{emb, write_map_fixture, write_symbol_map_fixture};
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
fn lookup_names_the_function_the_address_sits_in() {
    let map = write_symbol_map_fixture("lookup_shape.map");
    let v = json_of(&[
        "--json",
        "keil",
        "map",
        map.to_str().unwrap(),
        "--lookup",
        "0x900353F0",
    ]);

    assert_eq!(v["kind"], "table");
    assert_eq!(v["rows"][0][0], "0x900353F0");
    assert_eq!(v["rows"][0][1], "etharp_raw");
    assert_eq!(v["rows"][0][2], "+0x58");
    assert_eq!(v["rows"][0][3], "Thumb Code");
}

/// A fault dump usually carries more than one address worth translating.
#[test]
fn lookup_takes_a_comma_separated_list() {
    let map = write_symbol_map_fixture("lookup_multi.map");
    let v = json_of(&[
        "--json",
        "keil",
        "map",
        map.to_str().unwrap(),
        "--lookup",
        "0x90035398,0x90092948",
    ]);

    let rows = v["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{v}");
    assert_eq!(rows[0][1], "etharp_raw");
    assert_eq!(rows[1][1], "Image$$ER_SYS_INIT$$Base");
}

/// Without `--lookup` the command still reports totals and regions — the
/// lookup is an extra question, not a replacement for the summary.
#[test]
fn plain_map_output_is_unchanged() {
    let map = write_map_fixture("lookup_plain.map");
    let v = json_of(&["--json", "keil", "map", map.to_str().unwrap()]);

    assert_eq!(v["kind"], "sections");
    assert_eq!(v["sections"].as_array().unwrap().len(), 2, "{v}");
}

/// A typo has to fail loudly: silently looking up address 0 would answer
/// "below every symbol" and send the reader after the wrong thing.
#[test]
fn a_malformed_address_fails() {
    let map = write_symbol_map_fixture("lookup_bad.map");
    let out = emb(&[
        "keil",
        "map",
        map.to_str().unwrap(),
        "--lookup",
        "0xZZZZ",
    ]);

    assert!(!out.status.success(), "should have failed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("0xZZZZ"), "unhelpful error: {stderr}");
}

/// A map built without the linker's symbol listing has nothing to look up, and
/// saying so beats reporting "below every symbol" for every address.
#[test]
fn a_map_without_a_symbol_table_says_so() {
    let map = write_map_fixture("lookup_empty.map");
    let out = emb(&[
        "keil",
        "map",
        map.to_str().unwrap(),
        "--lookup",
        "0x08000000",
    ]);

    assert!(!out.status.success(), "should have failed");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("symbol table"), "unhelpful error: {stderr}");
}
