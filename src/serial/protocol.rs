/// Decode hex string to bytes ("4154" -> [0x41, 0x54])
pub fn decode_hex(hex: &str) -> anyhow::Result<Vec<u8>> {
    let hex = hex.trim();
    if hex.is_empty() {
        return Ok(Vec::new());
    }
    if !hex.len().is_multiple_of(2) {
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

/// Decode backslash escapes into bytes: `\r` `\n` `\t` `\0` `\e` `\\` `\xHH`.
///
/// Unknown escapes are kept literally, so a Windows path such as `C:\data`
/// survives unchanged. `\x` requires exactly two hex digits.
pub fn unescape(input: &str) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }

        let Some(next) = chars.next() else {
            out.push(b'\\');
            break;
        };

        match next {
            'r' => out.push(b'\r'),
            'n' => out.push(b'\n'),
            't' => out.push(b'\t'),
            '0' => out.push(0),
            'e' => out.push(0x1b),
            '\\' => out.push(b'\\'),
            'x' => {
                let hi = chars.next();
                let lo = chars.next();
                let (Some(hi), Some(lo)) = (hi, lo) else {
                    anyhow::bail!("incomplete \\x escape (need two hex digits)");
                };
                let digits: String = [hi, lo].iter().collect();
                let byte = u8::from_str_radix(&digits, 16)
                    .map_err(|_| anyhow::anyhow!("invalid \\x escape: \\x{digits}"))?;
                out.push(byte);
            }
            other => {
                // Unknown escape: keep the backslash and the character.
                out.push(b'\\');
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
        }
    }

    Ok(out)
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
        assert_eq!(decode_hex("").unwrap(), Vec::<u8>::new());
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
