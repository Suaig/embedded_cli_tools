//! Keil armlink .map file parser.
//!
//! Extracts the high-signal sections of a Keil (armlink) linker map:
//!   - Grand Totals (Code / RO / RW / ZI / Debug)
//!   - Total RO / RW / ROM size (human summary near the end)
//!   - Image entry point
//!   - Execution Regions (name, exec/load base, size, max, attributes)
//!
//! This is a best-effort text scanner: armlink's map format is line-oriented
//! and stable across MDK releases, so we avoid a full grammar and just pick
//! the lines we care about.

use std::path::Path;

use anyhow::{Context, Result};

/// Grand totals block (all values in bytes).
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct MapTotals {
    /// Code (inc. data) — column 0 of "Grand Totals"
    pub code: u64,
    /// RO Data — column 1
    pub ro_data: u64,
    /// RW Data — column 2
    pub rw_data: u64,
    /// ZI Data — column 3
    pub zi_data: u64,
    /// Debug — column 4
    pub debug: u64,
    /// Grand Totals — column 5
    pub grand: u64,
    /// "Total RO  Size (Code + RO Data)"
    pub ro_size: u64,
    /// "Total RW  Size (RW Data + ZI Data)"
    pub rw_size: u64,
    /// "Total ROM Size (Code + RO Data + RW Data)"
    pub rom_size: u64,
}

/// One execution region from the "Memory Map of the image" block.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecRegion {
    pub name: String,
    pub exec_base: u64,
    pub load_base: u64,
    pub size: u64,
    pub max: u64,
    pub attr: String,
}

/// Parsed map summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MapInfo {
    pub entry_point: u64,
    pub totals: MapTotals,
    pub regions: Vec<ExecRegion>,
}

/// Parse a Keil .map file from disk.
pub fn parse_map(path: &Path) -> Result<MapInfo> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("read map file: {}", path.display()))?;
    Ok(parse_map_text(&text))
}

/// Parse Keil .map content from an in-memory string.
pub fn parse_map_text(text: &str) -> MapInfo {
    let mut info = MapInfo {
        entry_point: 0,
        totals: MapTotals::default(),
        regions: Vec::new(),
    };

    for line in text.lines() {
        let t = line.trim_start();

        // "  Image Entry point : 0x080002cd"
        if t.starts_with("Image Entry point") {
            if let Some(hex) = t.split(':').nth(1) {
                info.entry_point = parse_hex(hex.trim()).unwrap_or(0);
            }
            continue;
        }

        // Grand Totals: "332408  59580  351596  213708  5772  8690839  Grand Totals"
        // (only the first match — the real totals line; ignore duplicates like
        // "ELF Image Totals" which do not end with the literal "Grand Totals")
        if t.ends_with("Grand Totals") {
            let nums: Vec<u64> = t
                .split_whitespace()
                .filter_map(|s| s.parse::<u64>().ok())
                .collect();
            if nums.len() >= 6 {
                info.totals.code = nums[0];
                info.totals.ro_data = nums[1];
                info.totals.rw_data = nums[2];
                info.totals.zi_data = nums[3];
                info.totals.debug = nums[4];
                info.totals.grand = nums[5];
            }
            continue;
        }

        // "    Total RO  Size (Code + RO Data)               684004 ( 667.97kB)"
        // Match the most specific prefix first: "Total ROM Size" must be
        // checked before "Total RO", otherwise strip_prefix("Total RO")
        // happily consumes the "Total ROM" line and overwrites ro_size.
        if t.starts_with("Total ROM Size") {
            if let Some(rest) = t.strip_prefix("Total ROM Size") {
                if let Some(b) = parse_total_size(rest) {
                    info.totals.rom_size = b;
                }
            }
        } else if let Some(rest) = t.strip_prefix("Total RO") {
            if let Some(b) = parse_total_size(rest) {
                info.totals.ro_size = b;
            }
        } else if let Some(rest) = t.strip_prefix("Total RW") {
            if let Some(b) = parse_total_size(rest) {
                info.totals.rw_size = b;
            }
        }

        // "    Execution Region ER_IROM1 (Exec base: 0x08000000, Load base: ...)"
        if t.starts_with("Execution Region") {
            if let Some(r) = parse_exec_region(t) {
                info.regions.push(r);
            }
        }
    }

    info
}

/// Parse a hex token like "0x08000000" or "08000000".
fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).ok()
}

/// "Size (Code + RO Data)               684004 ( 667.97kB)" -> 684004
/// (first decimal integer on the line, which is the byte count.)
fn parse_total_size(rest: &str) -> Option<u64> {
    rest.split_whitespace()
        .find_map(|t| t.parse::<u64>().ok())
}

/// Parse one "Execution Region NAME (key: val, key: val, ...)" line.
fn parse_exec_region(line: &str) -> Option<ExecRegion> {
    let after = line.strip_prefix("Execution Region")?;
    let paren_open = after.find('(')?;
    let name = after[..paren_open].trim().to_string();
    let inside = after[paren_open + 1..].trim_end_matches(')');

    let exec_base = field_hex(inside, "Exec base:")?;
    let load_base = field_hex(inside, "Load base:")?;
    let size = field_hex(inside, "Size:")?;
    let max = field_hex(inside, "Max:")?;
    // Attribute is the last comma-separated token inside the parens
    // (e.g. "ABSOLUTE", "ABSOLUTE, COMPRESSED[0x...]").
    let attr = inside.rsplit(',').next().unwrap_or("").trim().to_string();

    Some(ExecRegion {
        name,
        exec_base,
        load_base,
        size,
        max,
        attr,
    })
}

/// Find "Key: 0xVAL" inside a comma-separated field list and return VAL.
fn field_hex(s: &str, key: &str) -> Option<u64> {
    let idx = s.find(key)?;
    let rest = &s[idx + key.len()..];
    let token = rest.split(',').next()?.trim();
    parse_hex(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
  Image Entry point : 0x080002cd
  Load Region LR_IROM1 (Base: 0x08000000, Size: 0x000db2b0, Max: 0x00100000, ABSOLUTE, COMPRESSED[0x000a786c])

    Execution Region ER_IROM1 (Exec base: 0x08000000, Load base: 0x08000000, Size: 0x000a6fe4, Max: 0x00100000, ABSOLUTE)
    Execution Region RW_DTCM (Exec base: 0x20000000, Load base: 0x080a6fe4, Size: 0x00001d58, Max: 0x00020000, ABSOLUTE, COMPRESSED[0x00000204])

    332408      59580     351596     213708       5772    8690839   Grand Totals
    332408      59580     351596       2184       5772    8690839   ELF Image Totals (compressed)
    Total RO  Size (Code + RO Data)               684004 ( 667.97kB)
    Total RW  Size (RW Data + ZI Data)            219480 ( 214.34kB)
    Total ROM Size (Code + RO Data + RW Data)     686188 ( 670.11kB)
";

    #[test]
    fn parses_totals_and_entry() {
        let info = parse_map_text(SAMPLE);
        assert_eq!(info.entry_point, 0x0800_02cd);
        assert_eq!(info.totals.code, 332408);
        assert_eq!(info.totals.ro_data, 59580);
        assert_eq!(info.totals.rw_data, 351596);
        assert_eq!(info.totals.zi_data, 213708);
        assert_eq!(info.totals.grand, 8690839);
        assert_eq!(info.totals.ro_size, 684004);
        assert_eq!(info.totals.rw_size, 219480);
        assert_eq!(info.totals.rom_size, 686188);
    }

    #[test]
    fn parses_exec_regions() {
        let info = parse_map_text(SAMPLE);
        assert_eq!(info.regions.len(), 2);
        let er = &info.regions[0];
        assert_eq!(er.name, "ER_IROM1");
        assert_eq!(er.exec_base, 0x0800_0000);
        assert_eq!(er.size, 0x000a_6fe4);
        assert_eq!(er.max, 0x0010_0000);
        assert_eq!(er.attr, "ABSOLUTE");

        let dtcm = &info.regions[1];
        assert_eq!(dtcm.name, "RW_DTCM");
        assert_eq!(dtcm.exec_base, 0x2000_0000);
        assert_eq!(dtcm.load_base, 0x080a_6fe4);
        // attr is the last comma token, which here is the COMPRESSED[...] part
        assert!(dtcm.attr.starts_with("COMPRESSED"));
    }
}
