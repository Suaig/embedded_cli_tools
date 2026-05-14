pub mod human;
pub mod ai;
pub mod json;

use serde::Serialize;

/// Output format selection.
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputFormat {
    #[default]
    Human,
    Ai,
    Json,
}

/// A structured output item that can be rendered in any format.
/// Each variant maps to a different visual representation.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OutputValue {
    /// A single string message (e.g., "ok", "not implemented").
    Message(String),
    /// A list of key-value pairs (e.g., project info).
    KeyValue(Vec<(String, String)>),
    /// A table with headers and rows.
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// A list of items (single column).
    List(Vec<String>),
}

/// Render output in the selected format.
pub fn render(value: &OutputValue, format: OutputFormat) -> String {
    match format {
        OutputFormat::Human => human::render(value),
        OutputFormat::Ai => ai::render(value),
        OutputFormat::Json => json::render(value),
    }
}

/// Convenience: render and print to stdout.
pub fn print(value: &OutputValue, format: OutputFormat) {
    let text = render(value, format);
    print!("{text}");
}

/// Print a "not implemented" message for the given command name.
pub fn not_implemented(cmd: &str, format: OutputFormat) {
    let val = OutputValue::Message(format!("{cmd}: not implemented"));
    print(&val, format);
}
