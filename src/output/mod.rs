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
///
/// JSON carries a `kind` discriminator so a consumer can dispatch on one field
/// instead of guessing the shape from the command it ran. `Table` is the
/// exception to the wrapping: `headers`/`rows` stay at the top level, so adding
/// `kind` was backward compatible for it.
#[derive(Debug, Clone)]
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
    /// Several renderings for one command (`keil map`, `keil locate`).
    ///
    /// JSON has to stay a *single* document, so these are nested under one
    /// `kind: "sections"` envelope rather than printed one after another.
    Sections(Vec<OutputValue>),
}

impl Serialize for OutputValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        match self {
            OutputValue::Message(value) => {
                map.serialize_entry("kind", "message")?;
                map.serialize_entry("value", value)?;
            }
            OutputValue::KeyValue(entries) => {
                map.serialize_entry("kind", "kv")?;
                map.serialize_entry("entries", entries)?;
            }
            OutputValue::Table { headers, rows } => {
                map.serialize_entry("kind", "table")?;
                map.serialize_entry("headers", headers)?;
                map.serialize_entry("rows", rows)?;
            }
            OutputValue::List(items) => {
                map.serialize_entry("kind", "list")?;
                map.serialize_entry("items", items)?;
            }
            OutputValue::Sections(parts) => {
                map.serialize_entry("kind", "sections")?;
                map.serialize_entry("sections", parts)?;
            }
        }
        map.end()
    }
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
pub fn display(value: &OutputValue, format: OutputFormat) {
    let text = render(value, format);
    println!("{text}");
}

/// Print a "not implemented" message for the given command name.
pub fn not_implemented(cmd: &str, format: OutputFormat) {
    let val = OutputValue::Message(format!("{cmd}: not implemented"));
    display(&val, format);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn as_json(value: &OutputValue) -> Value {
        serde_json::from_str(&render(value, OutputFormat::Json)).expect("valid JSON")
    }

    #[test]
    fn json_is_self_describing() {
        assert_eq!(as_json(&OutputValue::Message("ok".into()))["kind"], "message");
        assert_eq!(as_json(&OutputValue::List(vec!["a".into()]))["kind"], "list");
        assert_eq!(
            as_json(&OutputValue::KeyValue(vec![("k".into(), "v".into())]))["kind"],
            "kv"
        );
        assert_eq!(
            as_json(&OutputValue::Table {
                headers: vec![],
                rows: vec![]
            })["kind"],
            "table"
        );
    }

    /// `headers`/`rows` staying at the top level is what makes adding `kind`
    /// backward compatible for tables.
    #[test]
    fn tables_keep_headers_and_rows_at_the_top_level() {
        let v = as_json(&OutputValue::Table {
            headers: vec!["Category".into()],
            rows: vec![vec!["RCC".into()]],
        });
        assert_eq!(v["headers"][0], "Category");
        assert_eq!(v["rows"][0][0], "RCC");
    }

    /// `emb ioc get` returns a bare value for an exact key and a list of entries
    /// otherwise. The discriminator is what makes that survivable for consumers.
    #[test]
    fn an_exact_value_and_a_list_of_entries_are_distinguishable() {
        let scalar = as_json(&OutputValue::Message("STM32H7R7L8Hx".into()));
        let pairs = as_json(&OutputValue::KeyValue(vec![(
            "GPIO.groupedBy".into(),
            "Show All".into(),
        )]));

        assert_ne!(scalar["kind"], pairs["kind"]);
        assert_eq!(scalar["value"], "STM32H7R7L8Hx");
        assert_eq!(pairs["entries"][0][0], "GPIO.groupedBy");
        assert_eq!(pairs["entries"][0][1], "Show All");
    }
}
