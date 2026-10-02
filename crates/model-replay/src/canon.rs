//! Canonical JSON and digests: what a print hashes.

use serde_json::Value;

/// Compact JSON with every object's keys sorted, so field order never changes a hash.
pub(crate) fn canonical(value: &Value) -> String {
    let mut out = String::new();
    write(value, &mut out);
    out
}

fn write(value: &Value, out: &mut String) {
    match value {
        Value::Array(items) => {
            out.push('[');
            for (at, item) in items.iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (at, key) in keys.into_iter().enumerate() {
                if at > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String(key.clone()).to_string());
                out.push(':');
                write(&map[key], out);
            }
            out.push('}');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

/// The BLAKE3 digest of `bytes` as 64 lowercase hex characters.
pub(crate) fn digest_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// A byte length as the cassette's `u64`.
pub(crate) fn len64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_sorted_at_every_depth() {
        let a: Value =
            serde_json::from_str(r#"{"b":1,"a":{"d":[{"y":1,"x":2}],"c":null}}"#).unwrap();
        assert_eq!(
            canonical(&a),
            r#"{"a":{"c":null,"d":[{"x":2,"y":1}]},"b":1}"#
        );
    }

    #[test]
    fn digest_is_blake3_hex() {
        assert_eq!(
            digest_hex(b""),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }
}
