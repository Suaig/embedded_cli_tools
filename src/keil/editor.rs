use std::fs;
use std::path::Path;

use anyhow::{Context, ensure};
use xmltree::{Element, EmitterConfig, XMLNode};

// ---------------------------------------------------------------------------
// XML tree helpers
// ---------------------------------------------------------------------------

/// Parse a .uvprojx file and return the root <Project> element.
fn load_xml(path: &Path) -> anyhow::Result<Element> {
    let file = fs::File::open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    Element::parse(file)
        .with_context(|| format!("failed to parse XML from {}", path.display()))
}

/// Write the XML tree back to file. Creates a .bak backup first.
fn save_xml(root: &Element, path: &Path) -> anyhow::Result<()> {
    // Read the current file first: its formatting is what we restore below.
    let original = fs::read_to_string(path).unwrap_or_default();

    // Create backup
    let bak_path = path.with_extension("uvprojx.bak");
    fs::copy(path, &bak_path)
        .with_context(|| format!("failed to create backup at {}", bak_path.display()))?;

    // Write to temp file first, then rename (atomic-ish)
    let tmp_path = path.with_extension("uvprojx.tmp");
    {
        let mut file = fs::File::create(&tmp_path)
            .with_context(|| format!("failed to create temp file {}", tmp_path.display()))?;
        // xmltree::Element::write() emits compact single-line XML that destroys
        // the original formatting (collapses a 3000-line .uvprojx to 1 line).
        // Use write_with_config with perform_indent + 2-space indent_string so
        // output stays readable and diff-friendly.
        let config = EmitterConfig {
            perform_indent: true,
            indent_string: std::borrow::Cow::Borrowed("  "),
            ..EmitterConfig::default()
        };
        root.write_with_config(&mut file, config)
            .with_context(|| "failed to write XML content")?;
    }

    // Put back the parts of the formatting the DOM cannot round-trip.
    {
        let serialized = fs::read_to_string(&tmp_path)
            .with_context(|| "failed to read temp file for format restoration")?;
        fs::write(&tmp_path, restore_keil_format(&serialized, &original))
            .with_context(|| "failed to write restored formatting")?;
    }

    // Verify the written file can be re-parsed
    {
        let verify_content = fs::read_to_string(&tmp_path)
            .with_context(|| "failed to read temp file for verification")?;
        let _ = super::parser::parse_project(&verify_content)
            .with_context(|| "verification failed: written file is not a valid .uvprojx")?;
    }

    // Replace original
    fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Format restoration
// ---------------------------------------------------------------------------

/// Undo the formatting a full DOM round trip loses.
///
/// xmltree rebuilds the file from its own tree, so even a net-zero edit drops
/// the XML declaration's `standalone="no"`, the namespace prefixes on
/// attributes, the trailing newline, and rewrites every empty element as
/// `<Tag />`. All four are recoverable from the original text with no
/// positional information. Measured on a real 1836-line `.uvprojx`, this takes a
/// net-zero round trip from 1512 changed lines down to 16 — the rest being
/// blank-line placement and attribute order, which the DOM does not preserve at
/// all.
///
/// (A positional approach — splicing into the original text — would close that
/// last gap, at the cost of rewriting every editor command.)
fn restore_keil_format(serialized: &str, original: &str) -> String {
    let mut text = restore_xml_declaration(serialized);
    text = restore_root_tag(&text, original);
    text = restore_empty_elements(&text, &always_self_closed_tags(original));
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// Keil writes `standalone="no"`; the DOM drops it.
fn restore_xml_declaration(text: &str) -> String {
    const KEIL_DECL: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="no" ?>"#;
    match text.find("?>") {
        Some(end) => format!("{KEIL_DECL}{}", &text[end + 2..]),
        None => text.to_string(),
    }
}

/// Reuse the original root open tag.
///
/// It carries the namespace declarations and the `xsi:`-prefixed attributes the
/// DOM drops. No editor command touches the root's attributes, so the original
/// tag is always the right one.
fn restore_root_tag(text: &str, original: &str) -> String {
    let (Some(original_tag), Some(serialized_tag)) =
        (root_open_tag(original), root_open_tag(text))
    else {
        return text.to_string();
    };
    match text.find(serialized_tag) {
        Some(at) => format!(
            "{}{}{}",
            &text[..at],
            original_tag,
            &text[at + serialized_tag.len()..]
        ),
        None => text.to_string(),
    }
}

/// The first element open tag in `text`, skipping the declaration and comments.
fn root_open_tag(text: &str) -> Option<&str> {
    let mut rest = text;
    loop {
        let start = rest.find('<')?;
        let after = &rest[start..];
        if after.starts_with("<?") || after.starts_with("<!") {
            rest = &after[after.find('>')? + 1..];
            continue;
        }
        let end = after.find('>')?;
        return Some(&after[..=end]);
    }
}

/// Write empty elements the way the original did.
///
/// Keil mixes `<Tag></Tag>` and `<Tag/>` in the same file, so rewriting every
/// empty element would itself be a formatting change. Tags the original
/// *always* writes self-closed keep that style — including the absent space
/// before `/>`, which the emitter always adds.
fn restore_empty_elements(text: &str, keep_self_closed: &std::collections::HashSet<&str>) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let trimmed = body.trim();
        let Some(name) = self_closed_name(trimmed) else {
            out.push_str(line);
            continue;
        };

        let indent = &body[..body.len() - body.trim_start().len()];
        let head = trimmed[..trimmed.len() - 3].trim_end(); // drop the " />"
        out.push_str(indent);
        out.push_str(head);
        if keep_self_closed.contains(name) {
            out.push_str("/>");
        } else {
            out.push_str("></");
            out.push_str(name);
            out.push('>');
        }
        out.push_str(&line[body.len()..]);
    }
    out
}

/// `<Name ... />` -> `Some("Name")`, for a line holding one empty element.
fn self_closed_name(body: &str) -> Option<&str> {
    if !body.starts_with('<') || !body.ends_with("/>") {
        return None;
    }
    if body.starts_with("<?") || body.starts_with("<!") {
        return None;
    }
    let inner = body[1..body.len() - 2].trim_end();
    let end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let name = &inner[..end];
    (!name.is_empty()).then_some(name)
}

/// `<Name></Name>` -> `Some("Name")`, for a line holding one empty element.
fn paired_name(body: &str) -> Option<&str> {
    let rest = body.strip_prefix('<')?;
    let close = rest.find("></")?;
    let name = &rest[..close];
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    (rest[close + 3..].strip_suffix('>') == Some(name)).then_some(name)
}

/// Tags the original writes self-closed and never as `<Tag></Tag>`.
fn always_self_closed_tags(text: &str) -> std::collections::HashSet<&str> {
    let mut self_closed = std::collections::HashSet::new();
    let mut paired = std::collections::HashSet::new();
    for line in text.lines() {
        let body = line.trim();
        if let Some(name) = self_closed_name(body) {
            self_closed.insert(name);
        }
        if let Some(name) = paired_name(body) {
            paired.insert(name);
        }
    }
    self_closed.difference(&paired).copied().collect()
}

/// Get the text content of an Element (from its Text children).
#[allow(dead_code)]
fn elem_text(elem: &Element) -> Option<String> {
    elem.get_text().map(|t| t.to_string())
}

/// Find the <Targets> element under root.
fn find_targets(root: &mut Element) -> anyhow::Result<&mut Element> {
    root.children
        .iter_mut()
        .filter_map(|n| n.as_mut_element())
        .find(|c| c.name == "Targets")
        .ok_or_else(|| anyhow::anyhow!("no <Targets> element found"))
}

/// Find a <Target> by name under <Targets>.
fn find_target_mut<'a>(
    targets: &'a mut Element,
    target_name: &str,
) -> anyhow::Result<&'a mut Element> {
    targets
        .children
        .iter_mut()
        .filter_map(|n| n.as_mut_element())
        .find(|c| {
            c.name == "Target"
                && c.get_child("TargetName")
                    .and_then(|tn| tn.get_text())
                    .map(|t| t == target_name)
                    .unwrap_or(false)
        })
        .ok_or_else(|| anyhow::anyhow!("target '{}' not found", target_name))
}

/// Navigate a chain of child tag names, creating missing nodes along the way.
/// Returns a mutable reference to the final node.
fn navigate_create<'a>(parent: &'a mut Element, path: &[&str]) -> &'a mut Element {
    let mut current = parent;
    for &tag in path {
        let pos = current.children.iter().position(|n| {
            n.as_element().map(|e| e.name == tag).unwrap_or(false)
        });
        if let Some(idx) = pos {
            current = current.children[idx].as_mut_element().unwrap();
        } else {
            let new_elem = Element::new(tag);
            current.children.push(XMLNode::Element(new_elem));
            let last = current.children.len() - 1;
            current = current.children[last].as_mut_element().unwrap();
        }
    }
    current
}

/// Navigate a chain of child tag names (read-only). Returns None if any step missing.
fn navigate<'a>(parent: &'a Element, path: &[&str]) -> Option<&'a Element> {
    let mut current = parent;
    for &tag in path {
        current = current.get_child(tag)?;
    }
    Some(current)
}

/// Get the text content of a child element, defaulting to empty string.
fn get_text_default(elem: &Element, tag: &str) -> String {
    elem.get_child(tag)
        .and_then(|c| c.get_text())
        .map(|t| t.to_string())
        .unwrap_or_default()
}

/// Set text content on an element. Clears all children and adds a single Text node.
fn set_element_text(elem: &mut Element, text: &str) {
    elem.children.clear();
    elem.children.push(XMLNode::Text(text.to_string()));
}

// ---------------------------------------------------------------------------
// Config key resolution
// ---------------------------------------------------------------------------

/// Map a dot-notation config key to an XML path (relative to <Target>).
/// Returns (xml_path_segments, is_bool_field).
fn resolve_config_path(key: &str) -> Option<(Vec<&'static str>, bool)> {
    match key {
        // ccompiler - Target level (AC5/AC6 selection)
        "ccompiler.ac6"           => Some((vec!["uAC6"], true)),
        "ccompiler.pcc"           => Some((vec!["pCCUsed"], false)),
        // ccompiler - C ads
        "ccompiler.optim"         => Some((vec!["TargetOption", "TargetArmAds", "Cads", "Optim"], false)),
        "ccompiler.otime"         => Some((vec!["TargetOption", "TargetArmAds", "Cads", "oTime"], true)),
        "ccompiler.c99"           => Some((vec!["TargetOption", "TargetArmAds", "Cads", "uC99"], true)),
        "ccompiler.gnu"           => Some((vec!["TargetOption", "TargetArmAds", "Cads", "uGnu"], true)),
        "ccompiler.wlevel"        => Some((vec!["TargetOption", "TargetArmAds", "Cads", "wLevel"], false)),
        "ccompiler.strict"        => Some((vec!["TargetOption", "TargetArmAds", "Cads", "Strict"], true)),
        "ccompiler.one_elf"       => Some((vec!["TargetOption", "TargetArmAds", "Cads", "OneElfS"], true)),
        "ccompiler.ropi"          => Some((vec!["TargetOption", "TargetArmAds", "Cads", "Ropi"], true)),
        "ccompiler.rwpi"          => Some((vec!["TargetOption", "TargetArmAds", "Cads", "Rwpi"], true)),
        "ccompiler.v6lang"        => Some((vec!["TargetOption", "TargetArmAds", "Cads", "v6Lang"], false)),
        "ccompiler.v6langp"       => Some((vec!["TargetOption", "TargetArmAds", "Cads", "v6LangP"], false)),
        "ccompiler.short_enums"   => Some((vec!["TargetOption", "TargetArmAds", "Cads", "vShortEn"], true)),
        "ccompiler.short_wchar"   => Some((vec!["TargetOption", "TargetArmAds", "Cads", "vShortWch"], true)),
        "ccompiler.misc"          => Some((vec!["TargetOption", "TargetArmAds", "Cads", "VariousControls", "MiscControls"], false)),
        // asm
        "asm.misc"                => Some((vec!["TargetOption", "TargetArmAds", "Aads", "VariousControls", "MiscControls"], false)),
        // output
        "output.hex"              => Some((vec!["TargetOption", "TargetCommonOption", "CreateHexFile"], true)),
        "output.name"             => Some((vec!["TargetOption", "TargetCommonOption", "OutputName"], false)),
        "output.debug_info"       => Some((vec!["TargetOption", "TargetCommonOption", "DebugInformation"], true)),
        // device
        "device.name"             => Some((vec!["TargetOption", "TargetCommonOption", "Device"], false)),
        // linker
        "linker.scatter"          => Some((vec!["TargetOption", "TargetArmAds", "LDads", "ScatterFile"], false)),
        "linker.misc"             => Some((vec!["TargetOption", "TargetArmAds", "LDads", "Misc"], false)),
        // memory
        "memory.irom.start"       => Some((vec!["TargetOption", "TargetArmAds", "ArmAdsMisc", "OnChipMemories", "IROM", "StartAddress"], false)),
        "memory.irom.size"        => Some((vec!["TargetOption", "TargetArmAds", "ArmAdsMisc", "OnChipMemories", "IROM", "Size"], false)),
        "memory.iram.start"       => Some((vec!["TargetOption", "TargetArmAds", "ArmAdsMisc", "OnChipMemories", "IRAM", "StartAddress"], false)),
        "memory.iram.size"        => Some((vec!["TargetOption", "TargetArmAds", "ArmAdsMisc", "OnChipMemories", "IRAM", "Size"], false)),
        "memory.xram.start"       => Some((vec!["TargetOption", "TargetArmAds", "ArmAdsMisc", "OnChipMemories", "XRAM", "StartAddress"], false)),
        "memory.xram.size"        => Some((vec!["TargetOption", "TargetArmAds", "ArmAdsMisc", "OnChipMemories", "XRAM", "Size"], false)),
        _ => None,
    }
}

/// All valid config keys for display in error messages.
const VALID_CONFIG_KEYS: &str = "\
ccompiler.ac6, ccompiler.pcc, ccompiler.optim, ccompiler.otime, \
ccompiler.c99, ccompiler.gnu, ccompiler.wlevel, ccompiler.strict, \
ccompiler.one_elf, ccompiler.ropi, ccompiler.rwpi, ccompiler.v6lang, \
ccompiler.v6langp, ccompiler.short_enums, ccompiler.short_wchar, ccompiler.misc, \
asm.misc, \
output.hex, output.name, output.debug_info, \
device.name, \
linker.scatter, linker.misc, \
memory.irom.start, memory.irom.size, memory.iram.start, memory.iram.size, \
memory.xram.start, memory.xram.size";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn config_set(
    path: &Path,
    target: &str,
    key: &str,
    value: &str,
) -> anyhow::Result<()> {
    let (xml_path, is_bool) = resolve_config_path(key)
        .ok_or_else(|| anyhow::anyhow!(
            "unknown config key: '{}'. Valid keys: {}", key, VALID_CONFIG_KEYS
        ))?;

    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let (leaf_tag, parent_path) = xml_path.split_last()
        .ok_or_else(|| anyhow::anyhow!("empty config path"))?;

    let parent = navigate_create(target_elem, parent_path);

    let final_value = if is_bool {
        match value {
            "1" | "true" | "yes" => "1",
            "0" | "false" | "no" => "0",
            _ => anyhow::bail!("key '{}' expects a boolean value (1/0, true/false, yes/no)", key),
        }
    } else {
        value
    };

    match parent.get_mut_child(*leaf_tag) {
        Some(elem) => {
            set_element_text(elem, final_value);
        }
        None => {
            let mut new_elem = Element::new(leaf_tag);
            set_element_text(&mut new_elem, final_value);
            parent.children.push(XMLNode::Element(new_elem));
        }
    }

    save_xml(&root, path)?;
    Ok(())
}

/// Find the <VariousControls> under <Cads> for a target (mutable).
fn find_c_various_mut(target_elem: &mut Element) -> anyhow::Result<&mut Element> {
    let path = ["TargetOption", "TargetArmAds", "Cads", "VariousControls"];
    let various = navigate(target_elem, &path[..path.len() - 1]);
    if various.is_none() {
        anyhow::bail!("Cads not found in target XML");
    }
    Ok(navigate_create(target_elem, &path))
}

pub fn defines_add(
    path: &Path,
    target: &str,
    macro_name: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let various = find_c_various_mut(target_elem)?;
    let current = get_text_default(various, "Define");
    let mut defs: Vec<String> = if current.is_empty() {
        Vec::new()
    } else {
        current.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    ensure!(!defs.iter().any(|d| d == macro_name),
        "define '{}' already exists", macro_name);

    defs.push(macro_name.to_string());
    let define_elem = various.get_mut_child("Define")
        .ok_or_else(|| anyhow::anyhow!("<Define> element not found"))?;
    set_element_text(define_elem, &defs.join(","));

    save_xml(&root, path)?;
    Ok(())
}

pub fn defines_remove(
    path: &Path,
    target: &str,
    macro_name: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let various = find_c_various_mut(target_elem)?;
    let current = get_text_default(various, "Define");
    let mut defs: Vec<String> = if current.is_empty() {
        Vec::new()
    } else {
        current.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    let original_len = defs.len();
    defs.retain(|d| d != macro_name);
    ensure!(defs.len() < original_len,
        "define '{}' not found", macro_name);

    let define_elem = various.get_mut_child("Define")
        .ok_or_else(|| anyhow::anyhow!("<Define> element not found"))?;
    set_element_text(define_elem, &defs.join(","));

    save_xml(&root, path)?;
    Ok(())
}

pub fn includes_add(
    path: &Path,
    target: &str,
    path_to_add: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let various = find_c_various_mut(target_elem)?;
    let current = get_text_default(various, "IncludePath");
    let mut paths: Vec<String> = if current.is_empty() {
        Vec::new()
    } else {
        current.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    ensure!(!paths.iter().any(|p| p == path_to_add),
        "include path '{}' already exists", path_to_add);

    paths.push(path_to_add.to_string());
    let inc_elem = various.get_mut_child("IncludePath")
        .ok_or_else(|| anyhow::anyhow!("<IncludePath> element not found"))?;
    set_element_text(inc_elem, &paths.join(";"));

    save_xml(&root, path)?;
    Ok(())
}

pub fn includes_remove(
    path: &Path,
    target: &str,
    path_to_remove: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let various = find_c_various_mut(target_elem)?;
    let current = get_text_default(various, "IncludePath");
    let mut paths: Vec<String> = if current.is_empty() {
        Vec::new()
    } else {
        current.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    };

    let original_len = paths.len();
    paths.retain(|p| p != path_to_remove);
    ensure!(paths.len() < original_len,
        "include path '{}' not found", path_to_remove);

    let inc_elem = various.get_mut_child("IncludePath")
        .ok_or_else(|| anyhow::anyhow!("<IncludePath> element not found"))?;
    set_element_text(inc_elem, &paths.join(";"));

    save_xml(&root, path)?;
    Ok(())
}

/// Find or create the <Groups> element under a target.
fn find_or_create_groups(target_elem: &mut Element) -> &mut Element {
    navigate_create(target_elem, &["Groups"])
}

/// Find a <Group> by name in the <Groups> container (mutable).
fn find_group_mut<'a>(groups: &'a mut Element, name: &str) -> Option<&'a mut Element> {
    groups.children.iter_mut()
        .filter_map(|n| n.as_mut_element())
        .find(|g| {
            g.name == "Group"
                && g.get_child("GroupName")
                    .and_then(|gn| gn.get_text())
                    .map(|t| t == name)
                    .unwrap_or(false)
        })
}

pub fn group_add(
    path: &Path,
    target: &str,
    name: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = find_or_create_groups(target_elem);

    ensure!(find_group_mut(groups, name).is_none(),
        "group '{}' already exists", name);

    let mut group = Element::new("Group");
    let mut group_name = Element::new("GroupName");
    set_element_text(&mut group_name, name);
    group.children.push(XMLNode::Element(group_name));

    let files = Element::new("Files");
    group.children.push(XMLNode::Element(files));

    groups.children.push(XMLNode::Element(group));

    save_xml(&root, path)?;
    Ok(())
}

pub fn group_remove(
    path: &Path,
    target: &str,
    name: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = navigate_create(target_elem, &["Groups"]);

    let original_len = groups.children.len();
    groups.children.retain(|g| {
        g.as_element().map(|e| {
            !(e.name == "Group"
                && e.get_child("GroupName")
                    .and_then(|gn| gn.get_text())
                    .map(|t| t == name)
                    .unwrap_or(false))
        }).unwrap_or(true) // retain non-element nodes
    });
    ensure!(groups.children.len() < original_len,
        "group '{}' not found", name);

    save_xml(&root, path)?;
    Ok(())
}

pub fn group_rename(
    path: &Path,
    target: &str,
    old_name: &str,
    new_name: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = navigate_create(target_elem, &["Groups"]);
    let group = find_group_mut(groups, old_name)
        .ok_or_else(|| anyhow::anyhow!("group '{}' not found", old_name))?;

    let gn = group.get_mut_child("GroupName")
        .ok_or_else(|| anyhow::anyhow!("group has no <GroupName> element"))?;
    set_element_text(gn, new_name);

    save_xml(&root, path)?;
    Ok(())
}

/// Detect file type from extension.
fn detect_file_type(filepath: &str) -> u8 {
    let ext = std::path::Path::new(filepath)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "c" => 1,
        "s" | "asm" => 2,
        "o" => 3,
        "a" | "lib" => 4,
        "h" => 5,
        _ => 6,
    }
}

pub fn file_add(
    path: &Path,
    target: &str,
    group: &str,
    filepath: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = navigate_create(target_elem, &["Groups"]);
    let group_elem = find_group_mut(groups, group)
        .ok_or_else(|| anyhow::anyhow!("group '{}' not found", group))?;

    // Ensure <Files> exists
    if group_elem.get_child("Files").is_none() {
        let files = Element::new("Files");
        group_elem.children.push(XMLNode::Element(files));
    }
    let files_elem = group_elem.get_mut_child("Files").unwrap();

    // Determine file name from path
    let file_name = std::path::Path::new(filepath)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("cannot extract filename from '{}'", filepath))?;

    // Check duplicate
    let already_exists = files_elem.children.iter()
        .filter_map(|n| n.as_element())
        .any(|f| {
            f.name == "File"
                && f.get_child("FileName")
                    .and_then(|fn_elem| fn_elem.get_text())
                    .map(|t| t == file_name)
                    .unwrap_or(false)
        });
    ensure!(!already_exists, "file '{}' already exists in group '{}'", file_name, group);

    let file_type = detect_file_type(filepath);

    // Build <File> element
    let mut file_elem = Element::new("File");

    let mut fn_node = Element::new("FileName");
    set_element_text(&mut fn_node, file_name);
    file_elem.children.push(XMLNode::Element(fn_node));

    let mut ft_node = Element::new("FileType");
    set_element_text(&mut ft_node, &file_type.to_string());
    file_elem.children.push(XMLNode::Element(ft_node));

    let mut fp_node = Element::new("FilePath");
    set_element_text(&mut fp_node, filepath);
    file_elem.children.push(XMLNode::Element(fp_node));

    files_elem.children.push(XMLNode::Element(file_elem));

    // Keil lets you register a source before creating it, so a missing path is
    // not an error — but staying silent about a typo is worse than saying so.
    // `<FilePath>` is relative to the .uvprojx directory.
    let resolved = path.parent().unwrap_or(Path::new(".")).join(filepath);
    if !resolved.exists() {
        eprintln!(
            "warning: '{}' does not exist yet (looked for {})",
            filepath,
            resolved.display()
        );
    }

    save_xml(&root, path)?;
    Ok(())
}

pub fn file_remove(
    path: &Path,
    target: &str,
    group: &str,
    filename: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = navigate_create(target_elem, &["Groups"]);
    let group_elem = find_group_mut(groups, group)
        .ok_or_else(|| anyhow::anyhow!("group '{}' not found", group))?;
    let files_elem = group_elem.get_mut_child("Files")
        .ok_or_else(|| anyhow::anyhow!("group '{}' has no <Files>", group))?;

    let original_len = files_elem.children.len();
    files_elem.children.retain(|f| {
        f.as_element().map(|e| {
            !(e.name == "File"
                && e.get_child("FileName")
                    .and_then(|fn_elem| fn_elem.get_text())
                    .map(|t| t == filename)
                    .unwrap_or(false))
        }).unwrap_or(true)
    });
    ensure!(files_elem.children.len() < original_len,
        "file '{}' not found in group '{}'", filename, group);

    save_xml(&root, path)?;
    Ok(())
}

/// Find a <File> element by name in a group's <Files> (mutable).
fn find_file_in_group_mut<'a>(
    group_elem: &'a mut Element,
    filename: &str,
) -> anyhow::Result<&'a mut Element> {
    let files_elem = group_elem.get_mut_child("Files")
        .ok_or_else(|| anyhow::anyhow!("group has no <Files> element"))?;

    files_elem.children.iter_mut()
        .filter_map(|n| n.as_mut_element())
        .find(|f| {
            f.name == "File"
                && f.get_child("FileName")
                    .and_then(|fn_elem| fn_elem.get_text())
                    .map(|t| t == filename)
                    .unwrap_or(false)
        })
        .ok_or_else(|| anyhow::anyhow!("file '{}' not found", filename))
}

pub fn file_exclude(
    path: &Path,
    target: &str,
    group: &str,
    filename: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = navigate_create(target_elem, &["Groups"]);
    let group_elem = find_group_mut(groups, group)
        .ok_or_else(|| anyhow::anyhow!("group '{}' not found", group))?;
    let file_elem = find_file_in_group_mut(group_elem, filename)?;

    // Remove existing IncludeInBuild if present, then add <IncludeInBuild>0</IncludeInBuild>
    file_elem.children.retain(|c| {
        c.as_element().map(|e| e.name != "IncludeInBuild").unwrap_or(true)
    });
    let mut iib = Element::new("IncludeInBuild");
    set_element_text(&mut iib, "0");
    file_elem.children.push(XMLNode::Element(iib));

    save_xml(&root, path)?;
    Ok(())
}

pub fn file_include(
    path: &Path,
    target: &str,
    group: &str,
    filename: &str,
) -> anyhow::Result<()> {
    let mut root = load_xml(path)?;
    let targets = find_targets(&mut root)?;
    let target_elem = find_target_mut(targets, target)?;

    let groups = navigate_create(target_elem, &["Groups"]);
    let group_elem = find_group_mut(groups, group)
        .ok_or_else(|| anyhow::anyhow!("group '{}' not found", group))?;
    let file_elem = find_file_in_group_mut(group_elem, filename)?;

    // Remove <IncludeInBuild> to restore default (included)
    let had = file_elem.children.iter().any(|c| {
        c.as_element().map(|e| e.name == "IncludeInBuild").unwrap_or(false)
    });
    ensure!(had, "file '{}' is already included in build (no <IncludeInBuild> override)", filename);

    file_elem.children.retain(|c| {
        c.as_element().map(|e| e.name != "IncludeInBuild").unwrap_or(true)
    });

    save_xml(&root, path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_project_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Project SchemaVersion="1.0">
  <Targets>
    <Target>
      <TargetName>TestTarget</TargetName>
      <ToolsetNumber>0x4</ToolsetNumber>
      <ToolsetName>ARM-ADS</ToolsetName>
      <uAC6>1</uAC6>
      <TargetOption>
        <TargetCommonOption>
          <Device>STM32H743VITx</Device>
          <Vendor>STMicroelectronics</Vendor>
          <OutputDirectory>.\Objects\</OutputDirectory>
          <OutputName>test_out</OutputName>
          <CreateExecutable>1</CreateExecutable>
          <CreateHexFile>1</CreateHexFile>
          <DebugInformation>1</DebugInformation>
        </TargetCommonOption>
        <TargetArmAds>
          <ArmAdsMisc>
            <OnChipMemories>
              <IROM><StartAddress>0x8000000</StartAddress><Size>0x100000</Size></IROM>
              <IRAM><StartAddress>0x20000000</StartAddress><Size>0x20000</Size></IRAM>
            </OnChipMemories>
          </ArmAdsMisc>
          <Cads>
            <Optim>2</Optim>
            <uC99>1</uC99>
            <uGnu>0</uGnu>
            <wLevel>3</wLevel>
            <VariousControls>
              <MiscControls>--diag_suppress=1</MiscControls>
              <Define>USE_HAL_DRIVER,STM32H743xx</Define>
              <IncludePath>../Core/Inc;../Drivers/CMSIS/Include</IncludePath>
            </VariousControls>
          </Cads>
          <LDads>
            <ScatterFile>linker.sct</ScatterFile>
          </LDads>
        </TargetArmAds>
      </TargetOption>
      <Groups>
        <Group>
          <GroupName>Application/User/Core</GroupName>
          <Files>
            <File>
              <FileName>main.c</FileName>
              <FileType>1</FileType>
              <FilePath>../Core/Src/main.c</FilePath>
            </File>
            <File>
              <FileName>excluded.c</FileName>
              <FileType>1</FileType>
              <FilePath>../Core/Src/excluded.c</FilePath>
              <IncludeInBuild>0</IncludeInBuild>
            </File>
          </Files>
        </Group>
        <Group>
          <GroupName>Drivers</GroupName>
          <Files>
            <File>
              <FileName>stm32h7xx_hal.c</FileName>
              <FileType>1</FileType>
              <FilePath>../Drivers/Src/stm32h7xx_hal.c</FilePath>
            </File>
          </Files>
        </Group>
      </Groups>
    </Target>
  </Targets>
</Project>"#
    }

    fn write_temp_project() -> tempfile::NamedTempFile {
        let mut tmp = tempfile::NamedTempFile::with_suffix(".uvprojx").unwrap();
        write!(tmp, "{}", sample_project_xml()).unwrap();
        tmp.flush().unwrap();
        tmp
    }

    #[test]
    fn test_config_set_optim() {
        let tmp = write_temp_project();
        let path = tmp.path();

        config_set(path, "TestTarget", "ccompiler.optim", "3").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].c_compiler.optimization, 3);
    }

    #[test]
    fn test_config_set_bool() {
        let tmp = write_temp_project();
        let path = tmp.path();

        config_set(path, "TestTarget", "ccompiler.gnu", "yes").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(proj.targets[0].c_compiler.gnu);
    }

    #[test]
    fn test_config_set_device_name() {
        let tmp = write_temp_project();
        let path = tmp.path();

        config_set(path, "TestTarget", "device.name", "STM32F407VGTx").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].device.name, "STM32F407VGTx");
    }

    #[test]
    fn test_config_set_memory() {
        let tmp = write_temp_project();
        let path = tmp.path();

        config_set(path, "TestTarget", "memory.irom.start", "0x08000000").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].memory.irom.start, "0x08000000");
    }

    #[test]
    fn test_config_set_unknown_key() {
        let tmp = write_temp_project();
        let result = config_set(tmp.path(), "TestTarget", "unknown.key", "val");
        assert!(result.is_err());
    }

    #[test]
    fn test_defines_add() {
        let tmp = write_temp_project();
        let path = tmp.path();

        defines_add(path, "TestTarget", "NEW_MACRO").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(proj.targets[0].c_compiler.defines.iter().any(|d| d == "NEW_MACRO"));
        assert_eq!(proj.targets[0].c_compiler.defines.len(), 3);
    }

    #[test]
    fn test_defines_add_duplicate() {
        let tmp = write_temp_project();
        let result = defines_add(tmp.path(), "TestTarget", "USE_HAL_DRIVER");
        assert!(result.is_err());
    }

    #[test]
    fn test_defines_remove() {
        let tmp = write_temp_project();
        let path = tmp.path();

        defines_remove(path, "TestTarget", "STM32H743xx").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(!proj.targets[0].c_compiler.defines.iter().any(|d| d == "STM32H743xx"));
        assert_eq!(proj.targets[0].c_compiler.defines.len(), 1);
    }

    #[test]
    fn test_defines_remove_not_found() {
        let tmp = write_temp_project();
        let result = defines_remove(tmp.path(), "TestTarget", "NO_SUCH_MACRO");
        assert!(result.is_err());
    }

    #[test]
    fn test_includes_add() {
        let tmp = write_temp_project();
        let path = tmp.path();

        includes_add(path, "TestTarget", "../New/Inc").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(proj.targets[0].c_compiler.include_paths.iter().any(|p| p == "../New/Inc"));
        assert_eq!(proj.targets[0].c_compiler.include_paths.len(), 3);
    }

    #[test]
    fn test_includes_remove() {
        let tmp = write_temp_project();
        let path = tmp.path();

        includes_remove(path, "TestTarget", "../Core/Inc").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(!proj.targets[0].c_compiler.include_paths.iter().any(|p| p == "../Core/Inc"));
        assert_eq!(proj.targets[0].c_compiler.include_paths.len(), 1);
    }

    #[test]
    fn test_group_add_and_remove() {
        let tmp = write_temp_project();
        let path = tmp.path();

        group_add(path, "TestTarget", "NewGroup").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].groups.len(), 3);

        group_remove(path, "TestTarget", "NewGroup").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].groups.len(), 2);
    }

    #[test]
    fn test_group_add_duplicate() {
        let tmp = write_temp_project();
        let result = group_add(tmp.path(), "TestTarget", "Drivers");
        assert!(result.is_err());
    }

    #[test]
    fn test_group_rename() {
        let tmp = write_temp_project();
        let path = tmp.path();

        group_rename(path, "TestTarget", "Drivers", "HAL_Drivers").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].groups[1].name, "HAL_Drivers");
    }

    #[test]
    fn test_file_add_and_remove() {
        let tmp = write_temp_project();
        let path = tmp.path();

        file_add(path, "TestTarget", "Drivers", "../Drivers/Src/new_driver.c").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        let drivers = &proj.targets[0].groups[1];
        assert_eq!(drivers.files.len(), 2);
        assert_eq!(drivers.files[1].name, "new_driver.c");
        assert_eq!(drivers.files[1].file_type, 1);

        file_remove(path, "TestTarget", "Drivers", "new_driver.c").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert_eq!(proj.targets[0].groups[1].files.len(), 1);
    }

    #[test]
    fn test_file_add_asm_type() {
        let tmp = write_temp_project();
        let path = tmp.path();

        file_add(path, "TestTarget", "Drivers", "../Drivers/Src/startup.s").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        let drivers = &proj.targets[0].groups[1];
        assert_eq!(drivers.files[1].file_type, 2);
    }

    #[test]
    fn test_file_exclude_and_include() {
        let tmp = write_temp_project();
        let path = tmp.path();

        // First, exclude main.c
        file_exclude(path, "TestTarget", "Application/User/Core", "main.c").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(!proj.targets[0].groups[0].files[0].included_in_build);

        // Now include it back
        file_include(path, "TestTarget", "Application/User/Core", "main.c").unwrap();

        let proj = super::super::parser::load_project(path).unwrap();
        assert!(proj.targets[0].groups[0].files[0].included_in_build);
    }

    #[test]
    fn test_file_include_already_included() {
        let tmp = write_temp_project();
        // main.c has no IncludeInBuild, so it's already included by default
        let result = file_include(tmp.path(), "TestTarget", "Application/User/Core", "main.c");
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_created() {
        let tmp = write_temp_project();
        let path = tmp.path();

        config_set(path, "TestTarget", "ccompiler.optim", "1").unwrap();

        let bak_path = path.with_extension("uvprojx.bak");
        assert!(bak_path.exists());
    }

    // -----------------------------------------------------------------------
    // Format restoration
    // -----------------------------------------------------------------------

    /// Keil's own output, including the two empty-element styles it mixes.
    const ORIGINAL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>\n\
<Project xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"project_projx.xsd\">\n\
  <SchemaVersion>2.1</SchemaVersion>\n\
  <MiscControls></MiscControls>\n\
  <apis/>\n\
</Project>\n";

    /// What xmltree emits for the same tree, with no edit applied.
    const SERIALIZED: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<Project xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" noNamespaceSchemaLocation=\"project_projx.xsd\">\n\
  <SchemaVersion>2.1</SchemaVersion>\n\
  <MiscControls />\n\
  <apis />\n\
</Project>";

    #[test]
    fn restores_the_declaration_root_tag_and_trailing_newline() {
        let out = restore_keil_format(SERIALIZED, ORIGINAL);
        assert!(
            out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\" ?>"),
            "{out}"
        );
        assert!(
            out.contains("xsi:noNamespaceSchemaLocation=\"project_projx.xsd\""),
            "the xsi: prefix must come back: {out}"
        );
        assert!(out.ends_with('\n'), "trailing newline missing: {out:?}");
    }

    #[test]
    fn restores_the_paired_style_for_tags_the_original_paired() {
        let out = restore_keil_format(SERIALIZED, ORIGINAL);
        assert!(out.contains("<MiscControls></MiscControls>"), "{out}");
    }

    #[test]
    fn keeps_self_closed_the_tags_the_original_always_self_closed() {
        let out = restore_keil_format(SERIALIZED, ORIGINAL);
        assert!(
            out.contains("<apis/>"),
            "the original wrote this tag self-closed; rewriting it is itself a change: {out}"
        );
    }

    #[test]
    fn a_paired_tag_is_not_mistaken_for_a_self_closed_one() {
        assert_eq!(self_closed_name("<Files></Files>"), None);
        assert_eq!(paired_name("<Files></Files>"), Some("Files"));

        // A tag Keil writes both ways keeps the paired style, which dominates.
        let mixed = "  <Files></Files>\n  <Files/>\n";
        assert!(!always_self_closed_tags(mixed).contains("Files"));
    }
}
