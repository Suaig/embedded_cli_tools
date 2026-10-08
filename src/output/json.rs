use crate::output::OutputValue;

/// Render output as JSON.
///
/// Ends with a newline like the human and ai renderers do, so that `display()`
/// can print any block verbatim and every format ends with exactly one newline.
pub fn render(value: &OutputValue) -> String {
    let mut out = serde_json::to_string_pretty(value).unwrap_or_else(|e| {
        format!("{{\"error\": \"serialization failed: {e}\"}}")
    });
    out.push('\n');
    out
}
