//! Keil armlink .map file parser.
//!
//! Extracts the high-signal sections of a Keil (armlink) linker map:
//!   - Grand Totals (Code / RO / RW / ZI / Debug)
//!   - Total RO / RW / ROM size (human summary near the end)
//!   - Image entry point
//!   - Execution Regions (name, exec/load base, size, max, attributes)
//!   - The Local and Global symbol tables (for address -> symbol lookup)
//!
//! This is a best-effort text scanner: armlink's map format is line-oriented
//! and stable across MDK releases, so we avoid a full grammar and just pick
//! the lines we care about.

use std::path::Path;

use anyhow::{Context, Result};

/// Grand totals block (all values in bytes).
///
/// armlink prints the row as six numbers under five headers, because
/// `Code (inc. data)` spans two columns:
///
/// ```text
///   Code (inc. data)   RO Data    RW Data    ZI Data      Debug
///  40828       4176       5408         40      14860     415344   Grand Totals
/// ```
///
/// `code` and the Code column's inline-data share the first header; the latter
/// is not part of any published total, so every field below maps to the column
/// whose right edge it is aligned with.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct MapTotals {
    /// Code — column 0
    pub code: u64,
    /// RO Data — column 2
    pub ro_data: u64,
    /// RW Data — column 3
    pub rw_data: u64,
    /// ZI Data — column 4
    pub zi_data: u64,
    /// Debug — column 5
    pub debug: u64,
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

/// One row of the map's "Local Symbols" / "Global Symbols" table.
///
/// armlink splits the table in two: functions and file-scope objects land under
/// Local Symbols, and only externally visible names reach Global Symbols. A
/// lookup that read just the Global half would miss almost every function, so
/// both tables are collected into one list.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Symbol {
    pub name: String,
    /// Symbol value. Code symbols are Thumb addresses, so bit 0 is set.
    pub value: u64,
    /// `Thumb Code` / `Code` / `Data` / `Number` / `Section` / ...
    pub kind: String,
    pub size: u64,
    /// e.g. `etharp.o(.text.etharp_raw)`; linker-generated entries have no
    /// parentheses (`anon$$obj.o ABSOLUTE`).
    pub object: String,
}

/// The symbol an address falls in, and how far into it the address is.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SymbolHit {
    pub symbol: Symbol,
    /// Bytes from the symbol's entry point to the queried address.
    pub offset: u64,
}

/// Collect the symbol tables of a Keil .map.
///
/// The tables are delimited by a `Local Symbols` / `Global Symbols` heading, a
/// `Symbol Name ... Object(Section)` column header, then one row per symbol:
///
/// ```text
///     Local Symbols
///
///     Symbol Name                              Value     Ov Type        Size  Object(Section)
///
///     etharp_raw                               0x90035399   Thumb Code   360  etharp.o(.text.etharp_raw)
/// ```
///
/// Rows are recognised by shape rather than by position, and a line that is
/// neither blank nor a row ends the table — that is what stops the scan at the
/// `====` rule before "Memory Map of the image".
pub fn parse_map_symbols(text: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let mut in_table = false;
    let mut in_rows = false;

    for line in text.lines() {
        let t = line.trim();

        if t == "Local Symbols" || t == "Global Symbols" {
            in_table = true;
            in_rows = false;
            continue;
        }
        if !in_table {
            continue;
        }
        if !in_rows {
            if t.starts_with("Symbol Name") {
                in_rows = true;
            }
            continue;
        }
        if t.is_empty() {
            continue;
        }
        match parse_symbol_line(t) {
            Some(s) => symbols.push(s),
            None => {
                in_table = false;
                in_rows = false;
            }
        }
    }

    symbols
}

/// Parse one symbol row: `name 0xVALUE Type... Size Object(Section)`.
///
/// Neither end of the row is fixed-width. The name may contain spaces
/// (`[Anonymous Symbol]`) and so may the object (`anon$$obj.o ABSOLUTE`), and
/// the type is one or two tokens (`Thumb Code`). Anchoring on the first `0x`
/// token fixes the name, and the first all-digit token after it fixes the size.
/// Rows from "Memory Map of the image" start with `0x` themselves and are
/// rejected because they leave an empty name.
fn parse_symbol_line(line: &str) -> Option<Symbol> {
    let tokens: Vec<&str> = line.split_whitespace().collect();

    let value_idx = tokens.iter().position(|t| t.starts_with("0x"))?;
    if value_idx == 0 {
        return None;
    }
    let name = tokens[..value_idx].join(" ");
    let value = parse_hex(tokens[value_idx])?;

    let rest = &tokens[value_idx + 1..];
    let size_idx = rest.iter().position(|t| t.parse::<u64>().is_ok())?;
    let kind = rest[..size_idx].join(" ");
    let size = rest[size_idx].parse::<u64>().ok()?;
    let object = rest[size_idx + 1..].join(" ");

    if kind.is_empty() || object.is_empty() {
        return None;
    }

    Some(Symbol {
        name,
        value,
        kind,
        size,
        object,
    })
}

fn is_code(kind: &str) -> bool {
    kind == "Code" || kind == "Thumb Code"
}

/// Find the symbol an address belongs to.
///
/// Thumb symbols carry bit 0 in their value and a program counter never does,
/// so both sides are masked before comparing.
///
/// The nearest symbol at or below the address wins. At the *same* address a
/// code symbol beats the others, because armlink also records a `Section`
/// marker (`[Anonymous Symbol]`) at every function's entry — without the
/// tie-break, `0x90035398` answers `[Anonymous Symbol]` instead of
/// `etharp_raw`. A nearer data symbol is still allowed to win: inside a data
/// region the honest answer is the data symbol that is actually there, and the
/// offset column makes it obvious when that is the case.
pub fn lookup_symbol(symbols: &[Symbol], addr: u64) -> Option<SymbolHit> {
    let target = addr & !1u64;
    let mut best: Option<&Symbol> = None;

    for s in symbols {
        let base = s.value & !1u64;
        if base > target {
            continue;
        }
        let better = match best {
            None => true,
            Some(b) => {
                let best_base = b.value & !1u64;
                if base != best_base {
                    base > best_base
                } else {
                    is_code(&s.kind) && !is_code(&b.kind)
                }
            }
        };
        if better {
            best = Some(s);
        }
    }

    best.map(|s| SymbolHit {
        offset: target - (s.value & !1u64),
        symbol: s.clone(),
    })
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

        // Grand Totals: "40828  4176  5408  40  14860  415344  Grand Totals"
        // (only the first match — the real totals line; ignore duplicates like
        // "ELF Image Totals" which do not end with the literal "Grand Totals")
        //
        // nums[1] is the Code column's inline data, which no published total
        // uses, so the remaining columns start at nums[2].
        if t.ends_with("Grand Totals") {
            let nums: Vec<u64> = t
                .split_whitespace()
                .filter_map(|s| s.parse::<u64>().ok())
                .collect();
            if nums.len() >= 6 {
                info.totals.code = nums[0];
                info.totals.ro_data = nums[2];
                info.totals.rw_data = nums[3];
                info.totals.zi_data = nums[4];
                info.totals.debug = nums[5];
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
pub fn parse_hex(s: &str) -> Option<u64> {
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
    // Everything after the "Max:" field is the attribute list, e.g. "ABSOLUTE"
    // or "ABSOLUTE, UNINIT, COMPRESSED[0x000001e4]". Keeping only the last token
    // dropped the attributes before it.
    let attr = match inside.find("Max:") {
        Some(idx) => match inside[idx..].find(',') {
            Some(comma) => inside[idx + comma + 1..].trim().to_string(),
            None => String::new(),
        },
        None => String::new(),
    };

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
        assert_eq!(info.totals.ro_data, 351596);
        assert_eq!(info.totals.rw_data, 213708);
        assert_eq!(info.totals.zi_data, 5772);
        assert_eq!(info.totals.debug, 8690839);
        assert_eq!(info.totals.ro_size, 684004);
        assert_eq!(info.totals.rw_size, 219480);
        assert_eq!(info.totals.rom_size, 686188);
    }

    /// A verbatim totals block from a real armlink map
    /// (`MDK-ARM/Boot/Fan_simulator_Boot/Fan_simulator_Boot.map`). UV4's own
    /// build log for the same image reports
    /// `Program Size: Code=40828 RO-data=5408 RW-data=40 ZI-data=14860`.
    const REAL_GRAND_TOTALS: &str = "\
      Code (inc. data)   RO Data    RW Data    ZI Data      Debug
     40828       4176       5408         40      14860     415344   Grand Totals
     40828       4176       5408         40      14860     415344   ELF Image Totals
     40828       4176       5408         40          0          0   ROM Totals
    Total RO  Size (Code + RO Data)                46236 (  45.15kB)
    Total RW  Size (RW Data + ZI Data)             14900 (  14.55kB)
    Total ROM Size (Code + RO Data + RW Data)      46276 (  45.19kB)
";

    /// The same block from a real map whose RW data is compressed into ROM
    /// (`MDK-ARM/Appli/Fan_simulator_Appli_SlotA/...SlotA.map`). Here the ROM
    /// Totals row's RW Data (0x40BE4) differs from the Grand Totals row's,
    /// which is what makes "Total ROM = Code + RO + RW" unusable as a check.
    const REAL_GRAND_TOTALS_COMPRESSED: &str = "\
      Code (inc. data)   RO Data    RW Data    ZI Data      Debug
    552912       6068      74804   33767584      43564    2071100   Grand Totals
    552912       6068      74804     265220      43564    2071100   ELF Image Totals (compressed)
    552912       6068      74804     265220          0          0   ROM Totals
    Total RO  Size (Code + RO Data)               627716 ( 613.00kB)
    Total RW  Size (RW Data + ZI Data)          33811148 (33018.70kB)
    Total ROM Size (Code + RO Data + RW Data)     892936 ( 872.01kB)
";

    /// armlink prints the Grand Totals row as six numbers under five headers,
    /// because `Code (inc. data)` spans two columns — the first holds the Code
    /// total, the second the code's inline data, which no published total uses:
    ///
    /// ```text
    ///   Code (inc. data)   RO Data    RW Data    ZI Data      Debug
    ///  40828       4176       5408         40      14860     415344
    /// ```
    ///
    /// Reading the row one column early makes all of RO/RW/ZI wrong, which the
    /// two totals armlink publishes on every map expose immediately:
    ///   Total RO = Code + RO Data
    ///   Total RW = RW Data + ZI Data
    #[test]
    fn grand_totals_columns_satisfy_armlink_identities() {
        let t = parse_map_text(REAL_GRAND_TOTALS).totals;

        assert_eq!(t.code, 40828);
        assert_eq!(t.ro_data, 5408);
        assert_eq!(t.rw_data, 40);
        assert_eq!(t.zi_data, 14860);

        assert_eq!(t.code + t.ro_data, t.ro_size);
        assert_eq!(t.rw_data + t.zi_data, t.rw_size);
    }

    /// `rom_size` must come from the map's own "Total ROM Size" line. Computing
    /// it as Code + RO + RW would agree on an uncompressed image but be wrong
    /// by tens of megabytes once RW data is compressed into ROM.
    #[test]
    fn total_rom_is_read_from_the_map_not_computed() {
        let t = parse_map_text(REAL_GRAND_TOTALS_COMPRESSED).totals;

        assert_eq!(t.code, 552912);
        assert_eq!(t.ro_data, 74804);
        assert_eq!(t.rw_data, 33767584);
        assert_eq!(t.zi_data, 43564);
        assert_eq!(t.debug, 2071100);

        assert_eq!(t.code + t.ro_data, t.ro_size);
        assert_eq!(t.rw_data + t.zi_data, t.rw_size);
        assert_eq!(t.rom_size, 892936);
        assert_ne!(t.code + t.ro_data + t.rw_data, t.rom_size);
    }

    /// Verbatim from a real map: this region carries three attributes, and
    /// keeping only the last one hid both `ABSOLUTE` and `UNINIT`.
    const REAL_MULTI_ATTR_REGION: &str = "\
    Execution Region RW_IRAM1_DTCM_HEAP (Exec base: 0x20001000, Load base: 0x9009a404, Size: 0x0000f000, Max: 0x0000f000, ABSOLUTE, UNINIT, COMPRESSED[0x000001e4])
";

    #[test]
    fn keeps_every_attribute_of_an_execution_region() {
        let info = parse_map_text(REAL_MULTI_ATTR_REGION);
        assert_eq!(info.regions.len(), 1);
        assert_eq!(
            info.regions[0].attr,
            "ABSOLUTE, UNINIT, COMPRESSED[0x000001e4]"
        );
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
        assert_eq!(dtcm.attr, "ABSOLUTE, COMPRESSED[0x00000204]");
    }

    /// Rows copied verbatim from a real map
    /// (`MDK-ARM/Listings/TX_SlotA.map`), including the two that are easy to
    /// get wrong: `[Anonymous Symbol]` (a name with a space) and
    /// `anon$$obj.o ABSOLUTE` (an object with a space and no parentheses).
    const SYMBOL_TABLES: &str = "\
    Local Symbols

    Symbol Name                              Value     Ov Type        Size  Object(Section)

    etharp_raw                               0x90035399   Thumb Code   360  etharp.o(.text.etharp_raw)
    [Anonymous Symbol]                       0x90035398   Section        0  etharp.o(.text.etharp_raw)
    etharp_input                             0x90035001   Thumb Code   296  etharp.o(.text.etharp_input)
    .L.str                                   0x90040000   Data          12  etharp.o(.rodata.str1.1)

==============================================================================

    Global Symbols

    Symbol Name                              Value     Ov Type        Size  Object(Section)

    Image$$ER_SYS_INIT$$Base                 0x90092948   Number         0  anon$$obj.o ABSOLUTE
    lwip_htons                               0x90030001   Thumb Code    24  def.o(.text.lwip_htons)

==============================================================================

Memory Map of the image

    Image Entry point : 0x90001000
    Execution Region ER_IROM1 (Exec base: 0x90001000, Load base: 0x90001000, Size: 0x000a6fe4, Max: 0x00100000, ABSOLUTE)
    0x90035398   0x90035398   0x00000168   Code   RO         5008    .text.etharp_raw    etharp.o
";

    fn symbol<'a>(symbols: &'a [Symbol], name: &str) -> &'a Symbol {
        symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no symbol named {name} in {:?}", symbols.len()))
    }

    /// A function is usually only in the Local table, so reading the Global
    /// half alone would answer "not found" for most addresses.
    #[test]
    fn symbols_are_collected_from_both_tables() {
        let symbols = parse_map_symbols(SYMBOL_TABLES);
        assert_eq!(symbols.len(), 6, "{symbols:?}");

        let raw = symbol(&symbols, "etharp_raw");
        assert_eq!(raw.value, 0x9003_5399);
        assert_eq!(raw.kind, "Thumb Code");
        assert_eq!(raw.size, 360);
        assert_eq!(raw.object, "etharp.o(.text.etharp_raw)");

        let anon = symbol(&symbols, "[Anonymous Symbol]");
        assert_eq!(anon.kind, "Section");

        let base = symbol(&symbols, "Image$$ER_SYS_INIT$$Base");
        assert_eq!(base.object, "anon$$obj.o ABSOLUTE");
        assert_eq!(base.size, 0);

        assert!(symbols.iter().all(|s| !s.name.is_empty()));
    }

    /// "Memory Map of the image" rows start with the address, which leaves the
    /// name column empty — the shape test that keeps them out of the table.
    #[test]
    fn memory_map_rows_are_not_symbols() {
        let symbols = parse_map_symbols(SYMBOL_TABLES);
        assert!(
            !symbols.iter().any(|s| s.object.starts_with(".text.etharp_raw ")),
            "{symbols:?}"
        );
    }

    #[test]
    fn lookup_lands_on_the_function_the_address_is_inside() {
        let symbols = parse_map_symbols(SYMBOL_TABLES);
        let hit = lookup_symbol(&symbols, 0x9003_53F0).expect("hit");
        assert_eq!(hit.symbol.name, "etharp_raw");
        assert_eq!(hit.offset, 0x58);
    }

    /// The Section marker sits at the function's entry address, so the two
    /// collide at offset 0 and the code symbol has to win.
    #[test]
    fn a_section_marker_does_not_shadow_the_function_at_the_same_address() {
        let symbols = parse_map_symbols(SYMBOL_TABLES);
        let hit = lookup_symbol(&symbols, 0x9003_5398).expect("hit");
        assert_eq!(hit.symbol.name, "etharp_raw");
        assert_eq!(hit.offset, 0);
    }

    /// Thumb values carry bit 0; a PC never does. Without masking, looking up
    /// the function's own entry point reports an offset of 1.
    #[test]
    fn the_thumb_bit_does_not_shift_the_offset() {
        let symbols = parse_map_symbols(SYMBOL_TABLES);
        for addr in [0x9003_5398, 0x9003_5399] {
            let hit = lookup_symbol(&symbols, addr).expect("hit");
            assert_eq!(hit.symbol.name, "etharp_raw", "at {addr:#x}");
            assert_eq!(hit.offset, 0, "at {addr:#x}");
        }
    }

    #[test]
    fn lookup_below_every_symbol_reports_nothing() {
        let symbols = parse_map_symbols(SYMBOL_TABLES);
        assert!(lookup_symbol(&symbols, 0x0000_0100).is_none());
    }

    #[test]
    fn an_empty_or_symbol_less_map_yields_no_symbols() {
        assert!(parse_map_symbols("").is_empty());
        assert!(parse_map_symbols("  Image Entry point : 0x08000000\n").is_empty());
    }
}
