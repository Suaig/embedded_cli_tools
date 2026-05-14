/// Decode hex string to bytes ("4154" -> [0x41, 0x54])
pub fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    let hex = hex.trim();
    if hex.is_empty() {
        return Ok(Vec::new());
    }
    if hex.len() % 2 != 0 {
        anyhow::bail!("hex string must have even length, got {}", hex.len());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|e| anyhow::anyhow!("invalid hex at position {}: {}", i, e))
        })
        .collect()
}

/// Encode bytes to hex string ([0x41, 0x54] -> "4154")
pub fn encode_hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{:02X}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_hex_basic() {
        assert_eq!(decode_hex("4154").unwrap(), vec![0x41, 0x54]);
    }

    #[test]
    fn test_decode_hex_empty() {
        assert_eq!(decode_hex("").unwrap(), Vec::new());
    }

    #[test]
    fn test_decode_hex_odd_length() {
        assert!(decode_hex("ABC").is_err());
    }

    #[test]
    fn test_decode_hex_invalid_char() {
        assert!(decode_hex("GG").is_err());
    }

    #[test]
    fn test_encode_hex_basic() {
        assert_eq!(encode_hex(&[0x41, 0x54]), "4154");
    }

    #[test]
    fn test_encode_hex_empty() {
        assert_eq!(encode_hex(&[]), "");
    }
}
