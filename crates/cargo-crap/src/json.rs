//! Minimal JSON value parser for `cargo metadata` output.

use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;

/// A JSON value needed to read workspace package fields.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// JSON `null`.
    Null,
    /// JSON boolean.
    Bool(bool),
    /// JSON number stored as `f64`.
    Number(f64),
    /// JSON string.
    String(String),
    /// JSON array.
    Array(Vec<Self>),
    /// JSON object.
    Object(BTreeMap<String, Self>),
}

impl Json {
    /// Parses `text` into a value.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not valid JSON.
    pub fn parse(text: &str) -> Result<Self> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        parser.skip_ws();
        let value = parser.value()?;
        parser.skip_ws();
        if parser.pos != parser.bytes.len() {
            return Err("trailing JSON after the document".into());
        }
        Ok(value)
    }

    /// Returns the object map when this value is an object.
    #[must_use]
    const fn as_object(&self) -> Option<&BTreeMap<String, Self>> {
        match self {
            Self::Object(map) => Some(map),
            _ => None,
        }
    }

    /// Returns the array slice when this value is an array.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Returns the string when this value is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// Looks up `key` in an object value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        self.as_object()?.get(key)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if !b.is_ascii_whitespace() {
                break;
            }
            self.pos += 1;
        }
    }

    fn peek(&self) -> Result<u8> {
        self.bytes.get(self.pos).copied().ok_or_else(|| "unexpected end of JSON".into())
    }

    fn bump(&mut self) -> Result<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Ok(b)
    }

    fn value(&mut self) -> Result<Json> {
        self.skip_ws();
        match self.peek()? {
            b'n' => self.ident(b"null", Json::Null),
            b't' => self.ident(b"true", Json::Bool(true)),
            b'f' => self.ident(b"false", Json::Bool(false)),
            b'"' => Ok(Json::String(self.string()?)),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            other => Err(format!("unexpected JSON byte {other:?}")),
        }
    }

    fn ident(&mut self, expected: &[u8], value: Json) -> Result<Json> {
        for &b in expected {
            if self.bump()? != b {
                return Err("invalid JSON literal".into());
            }
        }
        Ok(value)
    }

    fn array(&mut self) -> Result<Json> {
        self.bump()?;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek()? == b']' {
                self.pos += 1;
                break;
            }
            if !items.is_empty() {
                self.expect(b',')?;
                self.skip_ws();
            }
            items.push(self.value()?);
        }
        Ok(Json::Array(items))
    }

    fn object(&mut self) -> Result<Json> {
        self.bump()?;
        let mut map = BTreeMap::new();
        loop {
            self.skip_ws();
            if self.peek()? == b'}' {
                self.pos += 1;
                break;
            }
            if !map.is_empty() {
                self.expect(b',')?;
                self.skip_ws();
            }
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            map.insert(key, self.value()?);
        }
        Ok(Json::Object(map))
    }

    fn expect(&mut self, wanted: u8) -> Result<()> {
        let got = self.bump()?;
        if got == wanted {
            Ok(())
        } else {
            Err(format!("expected {} found {}", wanted as char, got as char))
        }
    }

    fn string(&mut self) -> Result<String> {
        self.expect(b'"')?;
        let mut raw = Vec::new();
        loop {
            match self.bump()? {
                b'"' => {
                    return String::from_utf8(raw).map_err(|_| "non-utf8 JSON string".into());
                }
                b'\\' => {
                    let ch = self.escape()?;
                    let mut buf = [0; 4];
                    raw.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                b if b.is_ascii_control() => {
                    return Err("control in JSON string".into());
                }
                b => raw.push(b),
            }
        }
    }

    fn escape(&mut self) -> Result<char> {
        match self.bump()? {
            b'"' => Ok('"'),
            b'\\' => Ok('\\'),
            b'/' => Ok('/'),
            b'b' => Ok('\u{0008}'),
            b'f' => Ok('\u{000c}'),
            b'n' => Ok('\n'),
            b'r' => Ok('\r'),
            b't' => Ok('\t'),
            b'u' => self.unicode_escape(),
            other => Err(format!("unknown JSON escape {}", other as char)),
        }
    }

    fn unicode_escape(&mut self) -> Result<char> {
        let mut code = 0_u32;
        for _ in 0..4 {
            let b = self.bump()?;
            let digit = hex_digit(b).ok_or_else(|| "invalid \\u escape".to_owned())?;
            code = (code << 4) | digit;
        }
        char::from_u32(code).ok_or_else(|| "invalid unicode escape".into())
    }

    fn number(&mut self) -> Result<Json> {
        let start = self.pos;
        if self.peek()? == b'-' {
            self.pos += 1;
        }
        self.digits()?;
        if self.bytes.get(self.pos) == Some(&b'.') {
            self.pos += 1;
            self.digits()?;
        }
        if matches!(self.bytes.get(self.pos), Some(&b'e' | &b'E')) {
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(&b'+' | &b'-')) {
                self.pos += 1;
            }
            self.digits()?;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "non-utf8 JSON number".to_owned())?;
        let n = text.parse().map_err(|_| "invalid JSON number".to_owned())?;
        Ok(Json::Number(n))
    }

    fn digits(&mut self) -> Result<()> {
        let start = self.pos;
        while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err("expected JSON digits".into());
        }
        Ok(())
    }
}

fn hex_digit(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some(u32::from(b - b'0')),
        b'a'..=b'f' => Some(u32::from(b - b'a' + 10)),
        b'A'..=b'F' => Some(u32::from(b - b'A' + 10)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object_string_array() {
        let json = Json::parse(r#"{"workspace_members":["a"],"n":1}"#);
        assert!(json.is_ok());
        let value = json.unwrap_or(Json::Null);
        let members = value.get("workspace_members").and_then(Json::as_array);
        assert!(members.is_some());
        let items = members.unwrap_or(&[]);
        assert_eq!(items.first().and_then(Json::as_str), Some("a"));
    }

    #[test]
    fn rejects_trailing_junk() {
        assert!(Json::parse("true false").is_err());
    }

    #[test]
    fn keeps_utf8_in_strings() {
        let json = Json::parse(r#"{"manifest_path":"/tmp/用户/Cargo.toml"}"#);
        assert!(json.is_ok());
        let value = json.unwrap_or(Json::Null);
        assert_eq!(
            value.get("manifest_path").and_then(Json::as_str),
            Some("/tmp/用户/Cargo.toml")
        );
    }
}
