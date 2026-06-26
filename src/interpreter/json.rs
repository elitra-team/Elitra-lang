use super::gc::{GcData, GcHeap};

use crate::interpreter::value::Value;

pub(crate) fn json_stringify(v: &Value, gc: &GcHeap) -> String {
    match v {
        Value::Nil => "null".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Int(n) => {
            format!("{}", n)
        }
        Value::Float(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Value::String(s) => {
            format!(
                "\"{}\"",
                s.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
            )
        }
        Value::List(h) => {
            if let GcData::List(items) = gc.get(*h) {
                let strs: Vec<String> = items.iter().map(|i| json_stringify(i, gc)).collect();
                return format!("[{}]", strs.join(","));
            }
            format!("\"{}\"", v)
        }
        Value::Dict(h) => {
            if let GcData::Dict(pairs) = gc.get(*h) {
                let strs: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{}:{}", json_stringify(k, gc), json_stringify(v, gc)))
                    .collect();
                return format!("{{{}}}", strs.join(","));
            }
            format!("\"{}\"", v)
        }
        Value::Set(h) => {
            if let GcData::Set(items) = gc.get(*h) {
                let strs: Vec<String> = items.iter().map(|i| json_stringify(i, gc)).collect();
                return format!("[{}]", strs.join(","));
            }
            format!("\"{}\"", v)
        }
        _ => format!("\"{}\"", v),
    }
}

pub(crate) fn json_pretty(v: &Value, indent: usize, gc: &GcHeap) -> String {
    let pad = "  ".repeat(indent);
    match v {
        Value::Nil => "null".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Int(n) => {
            format!("{}", n)
        }
        Value::Float(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        Value::String(s) => {
            format!(
                "\"{}\"",
                s.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
            )
        }
        Value::List(h) => {
            if let GcData::List(items) = gc.get(*h) {
                if items.is_empty() {
                    return "[]".into();
                }
                let inner: Vec<String> = items.iter().map(|i| json_pretty(i, indent + 1, gc)).collect();
                let child_pad = "  ".repeat(indent + 1);
                let lines: Vec<String> = inner.iter().map(|s| format!("{}{}", child_pad, s)).collect();
                return format!("[\n{}\n{}]", lines.join(",\n"), pad);
            }
            format!("\"{}\"", v)
        }
        Value::Dict(h) => {
            if let GcData::Dict(pairs) = gc.get(*h) {
                if pairs.is_empty() {
                    return "{}".into();
                }
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{}:{}", json_pretty(k, indent + 1, gc), json_pretty(v, indent + 1, gc)))
                    .collect();
                let child_pad = "  ".repeat(indent + 1);
                let lines: Vec<String> = inner.iter().map(|s| format!("{}{}", child_pad, s)).collect();
                return format!("{{\n{}\n{}}}", lines.join(",\n"), pad);
            }
            format!("\"{}\"", v)
        }
        Value::Set(h) => {
            if let GcData::Set(items) = gc.get(*h) {
                if items.is_empty() {
                    return "[]".into();
                }
                let inner: Vec<String> = items.iter().map(|i| json_pretty(i, indent + 1, gc)).collect();
                let child_pad = "  ".repeat(indent + 1);
                let lines: Vec<String> = inner.iter().map(|s| format!("{}{}", child_pad, s)).collect();
                return format!("[\n{}\n{}]", lines.join(",\n"), pad);
            }
            format!("\"{}\"", v)
        }
        _ => format!("\"{}\"", v),
    }
}

fn unescape_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(code) {
                            result.push(ch);
                        }
                    }
                }
                Some(c) => result.push(c),
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub(crate) fn json_parse(s: &str, gc: &mut GcHeap) -> Result<Value, String> {
    let s = s.trim();
    if s == "null" {
        return Ok(Value::Nil);
    }
    if s == "true" {
        return Ok(Value::Boolean(true));
    }
    if s == "false" {
        return Ok(Value::Boolean(false));
    }
    if let Ok(n) = s.parse::<f64>() {
        return Ok(Value::Float(n));
    }
    if s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        return Ok(Value::String(unescape_json(inner)));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::List(gc.alloc(GcData::List(Vec::new()))));
        }
        let mut items = Vec::new();
        let mut depth = 0;
        let mut start = 0;
        let mut in_str = false;
        for (i, ch) in inner.char_indices() {
            if in_str {
                if ch == '"' && !inner[..i].ends_with('\\') {
                    in_str = false;
                }
                continue;
            }
            match ch {
                '"' => in_str = true,
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    items.push(json_parse(inner[start..i].trim(), gc)?);
                    start = i + 1;
                }
                _ => {}
            }
        }
        items.push(json_parse(inner[start..].trim(), gc)?);
        return Ok(Value::List(gc.alloc(GcData::List(items))));
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return Ok(Value::Dict(gc.alloc(GcData::Dict(Vec::new()))));
        }
        let mut pairs = Vec::new();
        let mut depth = 0;
        let mut start = 0;
        let mut in_str = false;
        let mut in_key = true;
        let mut key = Value::Nil;
        for (i, ch) in inner.char_indices() {
            if in_str {
                if ch == '"' && !inner[..i].ends_with('\\') {
                    in_str = false;
                }
                continue;
            }
            match ch {
                '"' => in_str = true,
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                ':' if depth == 0 && in_key => {
                    key = json_parse(inner[start..i].trim(), gc)?;
                    start = i + 1;
                    in_key = false;
                }
                ',' if depth == 0 && !in_key => {
                    pairs.push((key.clone(), json_parse(inner[start..i].trim(), gc)?));
                    start = i + 1;
                    in_key = true;
                }
                _ => {}
            }
        }
        if !in_key {
            pairs.push((key, json_parse(inner[start..].trim(), gc)?));
        }
        return Ok(Value::Dict(gc.alloc(GcData::Dict(pairs))));
    }
    Err(format!("Cannot parse JSON: '{}'", s))
}

pub(crate) fn json_validate(s: &str, gc: &mut GcHeap) -> bool {
    json_parse(s, gc).is_ok()
}
