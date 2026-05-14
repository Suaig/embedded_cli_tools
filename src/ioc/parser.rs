use std::path::Path;

/// Parsed IOC file
#[derive(Debug)]
pub struct IocFile {
    /// All key-value pairs in original order
    pub entries: Vec<(String, String)>,
    /// Unique top-level prefixes (categories) in order of first appearance
    pub categories: Vec<String>,
}

impl IocFile {
    /// Get all entries matching a key prefix
    pub fn get_by_prefix(&self, prefix: &str) -> Vec<(&str, &str)> {
        self.entries
            .iter()
            .filter(|(k, _)| k == prefix || k.starts_with(&format!("{prefix}.")))
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// Get a single value by exact key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Check if a key exists
    #[allow(dead_code)]
    pub fn contains(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }
}

/// Parse IOC file content into structured representation
pub fn parse_ioc(content: &str) -> anyhow::Result<IocFile> {
    let mut entries = Vec::new();
    let mut categories = Vec::new();
    let mut seen_categories = std::collections::HashSet::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Split at first '='
        let Some(eq_pos) = line.find('=') else {
            continue;
        };
        let raw_key = &line[..eq_pos];
        let raw_value = &line[eq_pos + 1..];

        // Unescape the key: IOC uses backslash escaping for special chars
        let key = unescape_key(raw_key);
        // Values can contain \: for literal colon - unescape that too
        let value = unescape_value(raw_value);

        // Extract category (prefix before first dot)
        if let Some(dot_pos) = key.find('.') {
            let category = &key[..dot_pos];
            if seen_categories.insert(category.to_string()) {
                categories.push(category.to_string());
            }
        } else {
            // Key without dot is its own category
            if seen_categories.insert(key.clone()) {
                categories.push(key.clone());
            }
        }

        entries.push((key, value));
    }

    Ok(IocFile {
        entries,
        categories,
    })
}

/// Load and parse an IOC file from disk
pub fn load_ioc(path: &Path) -> anyhow::Result<IocFile> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {e}", path.display()))?;
    parse_ioc(&content)
}

/// Unescape IOC key: \SPACE -> SPACE, \: -> :, \\ -> \
fn unescape_key(s: &str) -> String {
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

/// Unescape IOC value: \: -> :
fn unescape_value(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = "\
#MicroXplorer Configuration settings - do not modify
File.Version=6
Mcu.CPN=STM32H743VIT6
Mcu.Family=STM32H7
board=custom
";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.entries.len(), 4);
        assert_eq!(ioc.get("File.Version"), Some("6"));
        assert_eq!(ioc.get("Mcu.CPN"), Some("STM32H743VIT6"));
        assert_eq!(ioc.get("board"), Some("custom"));
        assert_eq!(ioc.get("nonexistent"), None);
    }

    #[test]
    fn test_categories() {
        let content = "\
File.Version=6
Mcu.CPN=STM32H743VIT6
Mcu.Family=STM32H7
RCC.ADCFreq_Value=40312500
board=custom
";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.categories, vec!["File", "Mcu", "RCC", "board"]);
    }

    #[test]
    fn test_escaped_keys() {
        let content = "PA13\\ (JTMS/SWDIO).Mode=Serial_Wire\n";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.entries.len(), 1);
        assert_eq!(ioc.entries[0].0, "PA13 (JTMS/SWDIO).Mode");
        assert_eq!(ioc.entries[0].1, "Serial_Wire");
        // Category is the part before first dot
        assert_eq!(ioc.categories, vec!["PA13 (JTMS/SWDIO)"]);
    }

    #[test]
    fn test_escaped_values() {
        let content = "NVIC.BusFault_IRQn=true\\:1\\:1\\:true\\:false\\:true\\:false\\:false\\:false\n";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(
            ioc.entries[0].1,
            "true:1:1:true:false:true:false:false:false"
        );
    }

    #[test]
    fn test_get_by_prefix() {
        let content = "\
RCC.ADCFreq_Value=40312500
RCC.SYSCLKFreq_VALUE=480000000
RCC.SYSCLKSource=RCC_SYSCLKSOURCE_PLLCLK
Mcu.Family=STM32H7
";
        let ioc = parse_ioc(content).unwrap();
        let rcc = ioc.get_by_prefix("RCC");
        assert_eq!(rcc.len(), 3);
        assert_eq!(rcc[0], ("RCC.ADCFreq_Value", "40312500"));
        assert_eq!(rcc[1], ("RCC.SYSCLKFreq_VALUE", "480000000"));
    }

    #[test]
    fn test_contains() {
        let content = "Mcu.Family=STM32H7\n";
        let ioc = parse_ioc(content).unwrap();
        assert!(ioc.contains("Mcu.Family"));
        assert!(!ioc.contains("Mcu.Name"));
    }

    #[test]
    fn test_empty_lines_and_comments_skipped() {
        let content = "\
# comment line

File.Version=6

# another comment
board=custom
";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.entries.len(), 2);
    }

    #[test]
    fn test_lines_without_equals_skipped() {
        let content = "File.Version=6\nthis has no equals\nboard=custom\n";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.entries.len(), 2);
    }

    #[test]
    fn test_empty_value() {
        let content = "Mcu.UserConstants=\n";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.entries[0].1, "");
    }

    #[test]
    fn test_value_with_equals() {
        let content = "Some.Key=a=b=c\n";
        let ioc = parse_ioc(content).unwrap();
        assert_eq!(ioc.entries[0].1, "a=b=c");
    }
}
