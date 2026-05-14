use std::path::Path;

/// Set a key-value pair in the IOC file.
/// If key exists: update value. Otherwise: append new line.
/// Preserves comments, blank lines, and original order.
pub fn set(path: &Path, key: &str, value: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    let mut found = false;

    for line in &mut lines {
        if let Some(eq_pos) = line.find('=') {
            let raw_key = &line[..eq_pos];
            let line_key = unescape(raw_key);
            if line_key == key {
                // Preserve the original key formatting (with escapes)
                *line = format!("{}={}", raw_key, escape_value(value));
                found = true;
                break;
            }
        }
    }

    if !found {
        lines.push(format!("{}={}", escape_key(key), escape_value(value)));
    }

    // Write back with trailing newline
    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(path, out)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", path.display()))?;

    Ok(())
}

/// Remove a key from the IOC file.
/// Preserves all other content (comments, blank lines, order).
pub fn remove(path: &Path, key: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;

    let lines: Vec<String> = content
        .lines()
        .filter(|line| {
            if let Some(eq_pos) = line.find('=') {
                let raw_key = &line[..eq_pos];
                let line_key = unescape(raw_key);
                return line_key != key;
            }
            true
        })
        .map(|l| l.to_string())
        .collect();

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(path, out)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {e}", path.display()))?;

    Ok(())
}

/// Unescape IOC key/value: backslash + char -> char
fn unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(escaped) => result.push(escaped),
                None => result.push(c),
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Escape a key for IOC format: special chars that need backslash escaping
fn escape_key(key: &str) -> String {
    let mut result = String::with_capacity(key.len());
    for c in key.chars() {
        match c {
            ' ' | '(' | ')' | ':' | '=' | '\\' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

/// Escape a value for IOC format: colons need backslash escaping
fn escape_value(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            ':' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::NamedTempFile;

    fn make_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{content}").unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_set_existing_key() {
        let f = make_temp("Mcu.Family=STM32H7\nboard=custom\n");
        set(f.path(), "board", "MyBoard").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("board=MyBoard"));
        assert!(result.contains("Mcu.Family=STM32H7"));
        assert!(!result.contains("board=custom"));
    }

    #[test]
    fn test_set_new_key() {
        let f = make_temp("Mcu.Family=STM32H7\n");
        set(f.path(), "board", "custom").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("board=custom"));
        assert!(result.contains("Mcu.Family=STM32H7"));
    }

    #[test]
    fn test_set_preserves_comments() {
        let f = make_temp("# comment\nMcu.Family=STM32H7\n\nboard=custom\n");
        set(f.path(), "Mcu.Family", "STM32F4").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("# comment"));
        assert!(result.contains("Mcu.Family=STM32F4"));
        assert!(result.contains("board=custom"));
        // Verify blank line preserved
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.iter().any(|l| l.is_empty()));
    }

    #[test]
    fn test_set_escaped_key() {
        let f = make_temp("PA13\\ (JTMS/SWDIO).Mode=Serial_Wire\n");
        set(f.path(), "PA13 (JTMS/SWDIO).Mode", "Trace").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        // The original key formatting should be preserved
        assert!(result.contains("PA13\\ (JTMS/SWDIO).Mode=Trace"));
    }

    #[test]
    fn test_set_value_with_colon() {
        let f = make_temp("NVIC.BusFault_IRQn=true\n");
        set(f.path(), "NVIC.BusFault_IRQn", "true:1:1").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("NVIC.BusFault_IRQn=true\\:1\\:1"));
    }

    #[test]
    fn test_remove_key() {
        let f = make_temp("Mcu.Family=STM32H7\nboard=custom\n");
        remove(f.path(), "board").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(!result.contains("board="));
        assert!(result.contains("Mcu.Family=STM32H7"));
    }

    #[test]
    fn test_remove_nonexistent_key() {
        let f = make_temp("Mcu.Family=STM32H7\nboard=custom\n");
        remove(f.path(), "nonexistent").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert_eq!(result, "Mcu.Family=STM32H7\nboard=custom\n");
    }

    #[test]
    fn test_remove_preserves_comments() {
        let f = make_temp("# comment\nMcu.Family=STM32H7\n# another\nboard=custom\n");
        remove(f.path(), "Mcu.Family").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("# comment"));
        assert!(result.contains("# another"));
        assert!(result.contains("board=custom"));
        assert!(!result.contains("Mcu.Family"));
    }

    #[test]
    fn test_remove_escaped_key() {
        let f = make_temp("PA13\\ (JTMS/SWDIO).Mode=Serial_Wire\nMcu.Family=STM32H7\n");
        remove(f.path(), "PA13 (JTMS/SWDIO).Mode").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(!result.contains("PA13"));
        assert!(result.contains("Mcu.Family=STM32H7"));
    }

    #[test]
    fn test_escape_key_special_chars() {
        assert_eq!(escape_key("PA13 (JTMS/SWDIO).Mode"), "PA13\\ \\(JTMS/SWDIO\\).Mode");
    }

    #[test]
    fn test_escape_value_colons() {
        assert_eq!(escape_value("true:1:1"), "true\\:1\\:1");
    }
}
