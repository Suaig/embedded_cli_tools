use crate::output::OutputValue;

/// Render output as JSON.
pub fn render(value: &OutputValue) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| {
        format!("{{\"error\": \"serialization failed: {e}\"}}")
    })
}
