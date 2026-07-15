//! Read-only locator for Keil .uvprojx.
//!
//! Returns the line number + raw XML snippet for a requested element, so an
//! AI editor (which works on line + content replacement) can patch the file
//! surgically:
//!   1. `emb keil locate <path> -t <target> defines`   → start line + raw
//!   2. Read just those lines (offset/limit)
//!   3. Edit by content replacement
//!
//! This avoids both (a) feeding the whole 3000-line XML to the model, and
//! (b) xmltree's format-destroying rewrite. emb never writes the file here.

use anyhow::{Context, Result};
use roxmltree::{Document, Node};

#[derive(Debug, Clone)]
pub struct Snippet {
    /// 1-based, inclusive
    pub start_line: usize,
    /// 1-based, inclusive
    pub end_line: usize,
    /// Original XML text of the element (as-is on disk)
    pub raw: String,
}

/// Convert a byte offset in `text` to a 1-based line number.
fn byte_to_line(text: &str, offset: usize) -> usize {
    let off = offset.min(text.len());
    text[..off].bytes().filter(|&b| b == b'\n').count() + 1
}

fn snippet_of(text: &str, node: Node) -> Snippet {
    let range = node.range();
    let raw = text.get(range.clone()).unwrap_or("").to_string();
    Snippet {
        start_line: byte_to_line(text, range.start),
        // range.end points just past the closing token; back up to last content line
        end_line: byte_to_line(text, range.end.saturating_sub(1)),
        raw,
    }
}

/// Descend through a chain of child tag names. Returns None if any step missing.
fn navigate<'a, 'input>(node: Node<'a, 'input>, path: &[&str]) -> Option<Node<'a, 'input>> {
    let mut cur = node;
    for &tag in path {
        cur = cur.children().find(|c| c.has_tag_name(tag))?;
    }
    Some(cur)
}

fn find_target<'a, 'input>(doc: &'a Document<'input>, target_name: &str) -> Result<Node<'a, 'input>> {
    let root = doc.root_element();
    let targets = root
        .children()
        .find(|c| c.has_tag_name("Targets"))
        .context("no <Targets> element")?;
    for t in targets.children().filter(|c| c.has_tag_name("Target")) {
        let name = t
            .children()
            .find(|c| c.has_tag_name("TargetName"))
            .and_then(|n| n.text())
            .unwrap_or("");
        if name == target_name {
            return Ok(t);
        }
    }
    anyhow::bail!("target '{}' not found", target_name)
}

/// First descendant of `node` whose tag equals `key` (depth-first).
fn walk_for_tag<'a, 'input>(node: Node<'a, 'input>, key: &str) -> Option<Node<'a, 'input>> {
    for c in node.children() {
        if c.is_element() && c.has_tag_name(key) {
            return Some(c);
        }
        if let Some(n) = walk_for_tag(c, key) {
            return Some(n);
        }
    }
    None
}

/// Locate the C compiler `<Define>` element (comma-separated preprocessor macros).
pub fn defines(text: &str, target: &str) -> Result<Snippet> {
    let doc = Document::parse(text)?;
    let t = find_target(&doc, target)?;
    let node = navigate(
        t,
        &["TargetOption", "TargetArmAds", "Cads", "VariousControls", "Define"],
    )
    .context("C compiler <Define> not found")?;
    Ok(snippet_of(text, node))
}

/// Locate the C compiler `<IncludePath>` element (semicolon-separated paths).
pub fn includes(text: &str, target: &str) -> Result<Snippet> {
    let doc = Document::parse(text)?;
    let t = find_target(&doc, target)?;
    let node = navigate(
        t,
        &["TargetOption", "TargetArmAds", "Cads", "VariousControls", "IncludePath"],
    )
    .context("C compiler <IncludePath> not found")?;
    Ok(snippet_of(text, node))
}

/// Locate any config leaf element by its tag name (e.g. Optim, uAC6, ScatterFile,
/// CreateHexFile). Searches the whole target subtree.
pub fn config_key(text: &str, target: &str, key: &str) -> Result<Snippet> {
    let doc = Document::parse(text)?;
    let t = find_target(&doc, target)?;
    let node = walk_for_tag(t, key).with_context(|| format!("config key '{}' not found", key))?;
    Ok(snippet_of(text, node))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<?xml version=\"1.0\"?>\n<Project>\n  <Targets>\n    <Target>\n      <TargetName>App</TargetName>\n      <TargetOption>\n        <TargetArmAds>\n          <Cads>\n            <VariousControls>\n              <Define>USE_HAL_DRIVER,STM32H723xx</Define>\n              <IncludePath>../Inc;../Drivers</IncludePath>\n            </VariousControls>\n          </Cads>\n        </TargetArmAds>\n      </TargetOption>\n    </Target>\n  </Targets>\n</Project>\n";

    #[test]
    fn locates_define_with_line_number() {
        let snip = defines(SAMPLE, "App").unwrap();
        // <Define> is on the 10th line (1-based)
        assert_eq!(snip.start_line, 10);
        assert_eq!(snip.end_line, 10);
        assert!(snip.raw.contains("USE_HAL_DRIVER,STM32H723xx"));
    }

    #[test]
    fn locates_includepath() {
        let snip = includes(SAMPLE, "App").unwrap();
        assert_eq!(snip.start_line, 11);
        assert!(snip.raw.contains("../Inc;../Drivers"));
    }

    #[test]
    fn config_key_finds_any_tag() {
        // TargetName is a known leaf; walk should find it
        let snip = config_key(SAMPLE, "App", "TargetName").unwrap();
        assert!(snip.raw.contains("App"));
    }
}
