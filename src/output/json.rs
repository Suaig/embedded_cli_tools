use crate::output::OutputValue;

/// Render output as JSON.
pub fn render(value: &OutputValue) -> String {
    match value {
        OutputValue::Message(msg) => {
            serde_json::json!({"message": msg}).to_string()
        }
        _ => {
            // OutputValue derives Serialize, so we can serialize directly.
            match serde_json::to_string_pretty(value) {
                Ok(s) => s,
                Err(e) => serde_json::json!({"error": format!("serialization failed: {e}")}).to_string(),
            }
        }
    }
}
