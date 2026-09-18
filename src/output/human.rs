use crate::output::OutputValue;
use comfy_table::{Table, ContentArrangement, presets::UTF8_FULL};

/// Render output in human-friendly format with ASCII tables.
pub fn render(value: &OutputValue) -> String {
    match value {
        OutputValue::Message(msg) => format!("{msg}\n"),
        OutputValue::KeyValue(pairs) => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(vec!["Key", "Value"]);
            for (k, v) in pairs {
                table.add_row(vec![k.clone(), v.clone()]);
            }
            format!("{table}\n")
        }
        OutputValue::Table { headers, rows } => {
            let mut table = Table::new();
            table.load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);
            table.set_header(headers.clone());
            for row in rows {
                table.add_row(row.clone());
            }
            format!("{table}\n")
        }
        OutputValue::List(items) => {
            let mut out = String::new();
            for item in items {
                out.push_str(&format!("  - {item}\n"));
            }
            out
        }
        OutputValue::Sections(parts) => render_sections(parts),
    }
}

/// Parts of one command's output, separated by a blank line.
///
/// Every part already ends in a newline, so joining on "\n" reproduces exactly
/// what two consecutive `display()` calls used to print.
fn render_sections(parts: &[OutputValue]) -> String {
    let rendered: Vec<String> = parts.iter().map(render).collect();
    rendered.join("\n")
}
