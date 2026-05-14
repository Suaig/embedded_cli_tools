use crate::output::OutputValue;

/// Render output in AI-optimized compact format: key:value, one line per item.
pub fn render(value: &OutputValue) -> String {
    match value {
        OutputValue::Message(msg) => format!("{msg}\n"),
        OutputValue::KeyValue(pairs) => {
            let mut out = String::new();
            for (k, v) in pairs {
                out.push_str(&format!("{k}:{v}\n"));
            }
            out
        }
        OutputValue::Table { headers, rows } => {
            let header_line = headers.join("|");
            let mut out = format!("{header_line}\n");
            for row in rows {
                let line = row.join("|");
                out.push_str(&format!("{line}\n"));
            }
            out
        }
        OutputValue::List(items) => {
            let mut out = String::new();
            for item in items {
                out.push_str(&format!("{item}\n"));
            }
            out
        }
    }
}
