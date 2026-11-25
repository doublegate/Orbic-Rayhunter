//! A minimal, std-only JSON parser (orx-cli carries no dependencies). Enough to
//! read the daemon's trusted local API responses (`/api/qmdl-manifest`,
//! `/api/system-stats`): objects, arrays, strings, numbers, bools, null. It is a
//! reader, not a general library -- it rejects malformed input and does not try
//! to preserve number formatting.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }
    /// A non-negative number as u64 (sizes, counts); None if negative/fractional.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Num(n) if *n >= 0.0 && n.fract() == 0.0 && *n <= u64::MAX as f64 => {
                Some(*n as u64)
            }
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(v) => Some(v),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }
    /// Look up a key in an object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.get(key),
            _ => None,
        }
    }
}

/// Parse a complete JSON document; trailing non-whitespace is an error.
pub fn parse(input: &str) -> Result<Json, String> {
    let bytes = input.as_bytes();
    let mut p = Parser {
        b: bytes,
        s: input,
        i: 0,
    };
    p.skip_ws();
    let v = p.value(0)?;
    p.skip_ws();
    if p.i != bytes.len() {
        return Err(format!("trailing data at byte {}", p.i));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    s: &'a str, // same bytes as `b`, for O(1) scalar decode (input is valid UTF-8)
    i: usize,
}

const MAX_DEPTH: usize = 64;

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while let Some(&c) = self.b.get(self.i) {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err("nesting too deep".to_string());
        }
        self.skip_ws();
        match self.b.get(self.i) {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(&c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(&c) => Err(format!("unexpected byte {:?} at {}", c as char, self.i)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn literal(&mut self, word: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(word.as_bytes()) {
            self.i += word.len();
            Ok(v)
        } else {
            Err(format!("invalid literal at {}", self.i))
        }
    }

    fn object(&mut self, depth: usize) -> Result<Json, String> {
        self.i += 1; // {
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.b.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Json::Obj(map));
        }
        loop {
            self.skip_ws();
            if self.b.get(self.i) != Some(&b'"') {
                return Err(format!("expected object key at {}", self.i));
            }
            let key = self.string()?;
            self.skip_ws();
            if self.b.get(self.i) != Some(&b':') {
                return Err(format!("expected ':' at {}", self.i));
            }
            self.i += 1;
            let val = self.value(depth + 1)?;
            map.insert(key, val);
            self.skip_ws();
            match self.b.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Json::Obj(map));
                }
                _ => return Err(format!("expected ',' or '}}' at {}", self.i)),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Json, String> {
        self.i += 1; // [
        let mut vec = Vec::new();
        self.skip_ws();
        if self.b.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Json::Arr(vec));
        }
        loop {
            let val = self.value(depth + 1)?;
            vec.push(val);
            self.skip_ws();
            match self.b.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                }
                Some(b']') => {
                    self.i += 1;
                    return Ok(Json::Arr(vec));
                }
                _ => return Err(format!("expected ',' or ']' at {}", self.i)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            match self.b.get(self.i) {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    match self.b.get(self.i) {
                        Some(b'"') => out.push('"'),
                        Some(b'\\') => out.push('\\'),
                        Some(b'/') => out.push('/'),
                        Some(b'n') => out.push('\n'),
                        Some(b't') => out.push('\t'),
                        Some(b'r') => out.push('\r'),
                        Some(b'b') => out.push('\u{0008}'),
                        Some(b'f') => out.push('\u{000c}'),
                        Some(b'u') => {
                            let hex = self
                                .b
                                .get(self.i + 1..self.i + 5)
                                .ok_or_else(|| "truncated \\u escape".to_string())?;
                            let cp = u32::from_str_radix(
                                std::str::from_utf8(hex)
                                    .map_err(|_| "bad \\u escape".to_string())?,
                                16,
                            )
                            .map_err(|_| "bad \\u escape".to_string())?;
                            // Surrogate pairs are not handled; replace with U+FFFD.
                            out.push(char::from_u32(cp).unwrap_or('\u{fffd}'));
                            self.i += 4;
                        }
                        _ => return Err(format!("bad escape at {}", self.i)),
                    }
                    self.i += 1;
                }
                Some(&c) if c < 0x20 => {
                    return Err(format!("control char in string at {}", self.i));
                }
                Some(_) => {
                    // Decode exactly one scalar in O(1). `self.s` is the same
                    // already-valid UTF-8 as `self.b`, and `self.i` sits on a char
                    // boundary (we only advance by whole tokens/chars), so slicing
                    // is cheap and does NOT re-validate the rest of the document
                    // (the previous from_utf8(tail) was O(N) per char -> O(N^2)).
                    let ch = self.s[self.i..]
                        .chars()
                        .next()
                        .ok_or_else(|| "unexpected end of string".to_string())?;
                    out.push(ch);
                    self.i += ch.len_utf8();
                }
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        while let Some(&c) = self.b.get(self.i) {
            if c.is_ascii_digit() || c == b'.' || c == b'e' || c == b'E' || c == b'+' || c == b'-' {
                self.i += 1;
            } else {
                break;
            }
        }
        let s =
            std::str::from_utf8(&self.b[start..self.i]).map_err(|_| "bad number".to_string())?;
        s.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("invalid number {s:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_objects_arrays_scalars() {
        let v = parse(r#"{"a":1,"b":[true,null,"x"],"c":{"d":-2.5}}"#).unwrap();
        assert_eq!(v.get("a").unwrap().as_u64(), Some(1));
        let b = v.get("b").unwrap().as_array().unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], Json::Bool(true));
        assert!(b[1].is_null());
        assert_eq!(b[2].as_str(), Some("x"));
        assert_eq!(v.get("c").unwrap().get("d").unwrap().as_f64(), Some(-2.5));
    }

    #[test]
    fn string_escapes_and_unicode() {
        let v = parse(r#""a\nA\t\"z""#).unwrap();
        assert_eq!(v.as_str(), Some("a\nA\t\"z"));
    }

    #[test]
    fn manifest_shaped_input() {
        let v = parse(r#"{"entries":[{"name":"1795","qmdl_size_bytes":869535,"stop_reason":"UserRequested"}],"current_entry":{"name":"1796","qmdl_size_bytes":983942,"stop_reason":null}}"#).unwrap();
        let e = v.get("entries").unwrap().as_array().unwrap();
        assert_eq!(e[0].get("name").unwrap().as_str(), Some("1795"));
        assert_eq!(e[0].get("qmdl_size_bytes").unwrap().as_u64(), Some(869535));
        assert!(!e[0].get("stop_reason").unwrap().is_null());
        assert!(
            v.get("current_entry")
                .unwrap()
                .get("stop_reason")
                .unwrap()
                .is_null()
        );
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse("{").is_err());
        assert!(parse(r#"{"a":}"#).is_err());
        assert!(parse("[1,2,]").is_err());
        assert!(parse("1 2").is_err()); // trailing data
        assert!(parse("").is_err());
        assert!(parse("nul").is_err());
    }

    #[test]
    fn as_u64_rejects_negative_and_fractional() {
        assert_eq!(parse("-1").unwrap().as_u64(), None);
        assert_eq!(parse("1.5").unwrap().as_u64(), None);
        assert_eq!(parse("42").unwrap().as_u64(), Some(42));
    }
}
