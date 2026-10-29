use crate::output::OutputValue;

/// Escape the column separator so a value containing `|` cannot shift columns.
fn cell(value: &str) -> String {
    value.replace('|', "\\|")
}

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
            let header_line = headers.iter().map(|h| cell(h)).collect::<Vec<_>>().join("|");
            let mut out = format!("{header_line}\n");
            for row in rows {
                let line = row.iter().map(|c| cell(c)).collect::<Vec<_>>().join("|");
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
        // Sections are separated by a `---` line: raw payloads (XML snippets,
        // UV4 log tails) can contain blank lines, so an empty line is not a
        // reliable boundary.
        OutputValue::Sections(parts) => parts
            .iter()
            .map(render)
            .collect::<Vec<_>>()
            .join("---\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cells_escape_the_column_separator() {
        let value = OutputValue::Table {
            headers: vec!["Key".into(), "Value".into()],
            rows: vec![vec!["a".into(), "x|y".into()]],
        };
        assert_eq!(render(&value), "Key|Value\na|x\\|y\n");
    }

    #[test]
    fn sections_are_delimited_even_when_a_part_contains_blank_lines() {
        let value = OutputValue::Sections(vec![
            OutputValue::Message("first\n\nstill first".into()),
            OutputValue::Message("second".into()),
        ]);
        assert_eq!(render(&value), "first\n\nstill first\n---\nsecond\n");
    }
}
