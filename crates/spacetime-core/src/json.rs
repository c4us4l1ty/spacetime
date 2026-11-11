//! A tiny, allocation-light JSON reader/writer.
//!
//! The shell exchanges state with the engine over newline-delimited JSON. Rather
//! than pull in `serde` (which would require network access at build time), the
//! workspace ships this focused implementation: it parses the full JSON grammar
//! (including surrogate-pair escapes) and serialises deterministically.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// A JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Value>),
    Obj(BTreeMap<String, Value>),
}

impl Value {
    pub fn obj() -> Value {
        Value::Obj(BTreeMap::new())
    }

    /// Insert into an object; returns the receiver for chaining.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Value {
        if let Value::Obj(map) = &mut self {
            map.insert(key.to_string(), value.into());
        }
        self
    }

    /// Insert into an object in place.
    pub fn set(&mut self, key: &str, value: impl Into<Value>) {
        if let Value::Obj(map) = self {
            map.insert(key.to_string(), value.into());
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> { self.as_f64().map(|n| n as u64) }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(items) => Some(items),
            _ => None,
        }
    }

    /// Convenience: object field as string.
    pub fn str_of(&self, key: &str) -> &str {
        self.get(key).and_then(Value::as_str).unwrap_or("")
    }

    /// Convenience: object field as u64.
    pub fn u64_of(&self, key: &str) -> u64 {
        self.get(key).and_then(Value::as_u64).unwrap_or(0)
    }

    /// Convenience: object field as bool.
    pub fn bool_of(&self, key: &str) -> bool {
        self.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    /// Parse a complete JSON document.
    pub fn parse(input: &str) -> Result<Value, String> {
        let mut parser = Parser {
            bytes: input.as_bytes(),
            pos: 0,
        };
        let value = parser.value()?;
        parser.ws();
        if parser.pos != parser.bytes.len() {
            return Err(format!("trailing characters at byte {}", parser.pos));
        }
        Ok(value)
    }

    /// Compact serialisation.
    ///
    /// Kept as `encode` rather than an inherent `to_string`: [`std::fmt::Display`]
    /// is implemented below, so the blanket `ToString` implementation supplies
    /// `to_string()` for free. Defining an inherent `to_string` as well would
    /// shadow the trait method (`inherent_to_string_shadow_display`), which CI
    /// turns into a hard error with `-D warnings`.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// Pretty serialisation with two-space indent.
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        self.write_pretty(&mut out, 0);
        out
    }

    fn write(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(true) => out.push_str("true"),
            Value::Bool(false) => out.push_str("false"),
            Value::Num(n) => write_number(*n, out),
            Value::Str(s) => write_string(s, out),
            Value::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Value::Obj(map) => {
                out.push('{');
                for (i, (key, value)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }

    fn write_pretty(&self, out: &mut String, depth: usize) {
        let pad = |out: &mut String, depth: usize| {
            for _ in 0..depth {
                out.push_str("  ");
            }
        };
        match self {
            Value::Arr(items) if !items.is_empty() => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    pad(out, depth + 1);
                    item.write_pretty(out, depth + 1);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push(']');
            }
            Value::Obj(map) if !map.is_empty() => {
                out.push_str("{\n");
                for (i, (key, value)) in map.iter().enumerate() {
                    pad(out, depth + 1);
                    write_string(key, out);
                    out.push_str(": ");
                    value.write_pretty(out, depth + 1);
                    if i + 1 < map.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                pad(out, depth);
                out.push('}');
            }
            other => other.write(out),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.encode())
    }
}

fn write_number(n: f64, out: &mut String) {
    if n.is_finite() && n.fract().abs() < f64::EPSILON && n.abs() < 9.007_199_254_740_992e15 {
        let _ = write!(out, "{}", n as i64);
    } else if n.is_finite() {
        let _ = write!(out, "{n}");
    } else {
        out.push_str("null");
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn ws(&mut self) {
        while let Some(b) = self.bytes.get(self.pos) {
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!(
                "expected '{}' at byte {}",
                byte as char, self.pos
            ))
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.number(),
            Some(other) => Err(format!("unexpected '{}' at byte {}", other as char, self.pos)),
            None => Err("unexpected end of input".into()),
        }
    }

    fn literal(&mut self, token: &str, value: Value) -> Result<Value, String> {
        if self.bytes[self.pos..].starts_with(token.as_bytes()) {
            self.pos += token.len();
            Ok(value)
        } else {
            Err(format!("invalid literal at byte {}", self.pos))
        }
    }

    fn object(&mut self) -> Result<Value, String> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Value::Obj(map));
        }
        loop {
            self.ws();
            let key = self.string()?;
            self.ws();
            self.expect(b':')?;
            let value = self.value()?;
            map.insert(key, value);
            self.ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Value::Obj(map));
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }

    fn array(&mut self) -> Result<Value, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Value::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.ws();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Value::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let byte = self
                .peek()
                .ok_or_else(|| "unterminated string".to_string())?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let esc = self
                        .peek()
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) {
                                // high surrogate: expect a following low surrogate
                                if self.peek() == Some(b'\\')
                                    && self.bytes.get(self.pos + 1) == Some(&b'u')
                                {
                                    self.pos += 2;
                                    let lo = self.hex4()?;
                                    if (0xDC00..0xE000).contains(&lo) {
                                        0x1_0000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                                    } else {
                                        return Err("invalid low surrogate".into());
                                    }
                                } else {
                                    return Err("unpaired high surrogate".into());
                                }
                            } else {
                                hi
                            };
                            out.push(
                                char::from_u32(code)
                                    .ok_or_else(|| "invalid code point".to_string())?,
                            );
                        }
                        other => {
                            return Err(format!("bad escape '\\{}'", other as char));
                        }
                    }
                }
                _ => {
                    // Copy one UTF-8 scalar.
                    let start = self.pos;
                    let len = utf8_len(byte);
                    let end = (start + len).min(self.bytes.len());
                    let text = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|e| format!("invalid utf-8: {e}"))?;
                    out.push_str(text);
                    self.pos = end;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let slice = self
            .bytes
            .get(self.pos..self.pos + 4)
            .ok_or_else(|| "truncated \\u escape".to_string())?;
        let text = std::str::from_utf8(slice).map_err(|e| e.to_string())?;
        let value = u32::from_str_radix(text, 16).map_err(|e| e.to_string())?;
        self.pos += 4;
        Ok(value)
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|e| e.to_string())?;
        text.parse::<f64>()
            .map(Value::Num)
            .map_err(|e| format!("bad number '{text}': {e}"))
    }
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(s)
    }
}
impl From<u64> for Value {
    fn from(n: u64) -> Value {
        Value::Num(n as f64)
    }
}
impl From<u32> for Value {
    fn from(n: u32) -> Value {
        Value::Num(n as f64)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Value {
        Value::Num(n as f64)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Value {
        Value::Num(n)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}
impl From<Vec<Value>> for Value {
    fn from(items: Vec<Value>) -> Value {
        Value::Arr(items)
    }
}
impl From<Vec<String>> for Value {
    fn from(items: Vec<String>) -> Value {
        Value::Arr(items.into_iter().map(Value::Str).collect())
    }
}
impl From<BTreeMap<String, Value>> for Value {
    fn from(map: BTreeMap<String, Value>) -> Value {
        Value::Obj(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_nested() {
        let text = r#"{"a":1,"b":[true,null,"x\ny"],"c":{"d":-2.5e3}}"#;
        let value = Value::parse(text).unwrap();
        assert_eq!(value.get("a").and_then(Value::as_u64), Some(1));
        assert_eq!(value.get("c").and_then(|c| c.get("d")).and_then(Value::as_f64), Some(-2500.0));
        let again = Value::parse(&value.to_string()).unwrap();
        assert_eq!(again, value);
    }

    #[test]
    fn escapes_and_unicode() {
        let value = Value::parse(r#""héllo \u00e9 \uD83D\uDE80 ""#).unwrap();
        assert_eq!(value, Value::Str("héllo é 🚀 ".into()));
        let encoded = Value::Str("a\"b\\c\nd".into()).to_string();
        assert_eq!(Value::parse(&encoded).unwrap(), Value::Str("a\"b\\c\nd".into()));
    }

    #[test]
    fn integers_stay_integers() {
        let v = Value::obj().with("n", 42u64).with("big", 1e18f64);
        let text = v.to_string();
        assert_eq!(text, r#"{"big":1000000000000000000,"n":42}"#);
    }

    #[test]
    fn rejects_garbage() {
        assert!(Value::parse("{").is_err());
        assert!(Value::parse("[1,]").is_err());
        assert!(Value::parse("nul").is_err());
        assert!(Value::parse("").is_err());
    }
}
