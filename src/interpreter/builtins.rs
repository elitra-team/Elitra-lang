use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::gc::{GcData, GcHeap};
use crate::interpreter::json::{json_parse, json_stringify, json_validate};
use crate::interpreter::value::Value;

pub(crate) fn register_builtins(globals: &mut HashMap<String, Value>) {
    for name in &[
        "len", "str", "int", "float", "bool", "type", "input", "abs", "sin", "cos", "sqrt",
        "say", "shout", "read", "write", "lines", "assert", "split", "trim", "upper", "lower",
        "contains", "replace", "push", "pop", "sort", "reverse", "join", "floor", "ceil", "round",
        "max", "min", "pow", "log", "exp", "json_encode", "json_decode", "json_validate", "clock", "exit",
        "map", "filter", "fold", "take", "collect", "iter", "set", "as_trait",
        "panic", "unreachable",
        "Ok", "Err", "Some",
    ] {
        globals.insert(name.to_string(), Value::BuiltinFn(name.to_string()));
    }
}

pub(crate) fn call_builtin(name: &str, evaled: Vec<Value>, gc: &mut GcHeap) -> Result<Value, String> {
    match name {
        "len" => {
            if evaled.len() != 1 {
                return Err("len() expects 1 arg".into());
            }
            match &evaled[0] {
                Value::List(h) => {
                    if let GcData::List(items) = gc.get(*h) {
                        Ok(Value::Int(items.len() as i64))
                    } else {
                        unreachable!()
                    }
                }
                Value::Set(h) => {
                    if let GcData::Set(items) = gc.get(*h) {
                        Ok(Value::Int(items.len() as i64))
                    } else {
                        unreachable!()
                    }
                }
                Value::String(s) => Ok(Value::Int(s.len() as i64)),
                Value::Dict(h) => {
                    if let GcData::Dict(pairs) = gc.get(*h) {
                        Ok(Value::Int(pairs.len() as i64))
                    } else {
                        unreachable!()
                    }
                }
                Value::Range(a, b) => Ok(Value::Float((b - a).max(0.0))),
                v => Err(format!("len() not for '{}'", v.type_name())),
            }
        }
        "str" => {
            if evaled.len() != 1 {
                return Err("str() expects 1 arg".into());
            }
            Ok(Value::String(evaled[0].to_string()))
        }
        "int" => {
            if evaled.len() != 1 {
                return Err("int() expects 1 arg".into());
            }
            match &evaled[0] {
                Value::Int(n) => Ok(Value::Int(*n)),
                Value::Float(n) => Ok(Value::Int(n.floor() as i64)),
                Value::String(s) => s
                    .parse::<f64>()
                    .map(|n| Value::Int(n.floor() as i64))
                    .map_err(|_| format!("Cannot parse '{}' as int", s)),
                Value::Boolean(true) => Ok(Value::Int(1)),
                Value::Boolean(false) => Ok(Value::Int(0)),
                v => Err(format!("Cannot convert '{}' to int", v)),
            }
        }
        "float" => {
            if evaled.len() != 1 {
                return Err("float() expects 1 arg".into());
            }
            match &evaled[0] {
                Value::Int(n) => Ok(Value::Float(*n as f64)),
                Value::Float(n) => Ok(Value::Float(*n)),
                Value::String(s) => s
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| format!("Cannot parse '{}' as float", s)),
                v => Err(format!("Cannot convert '{}' to float", v)),
            }
        }
        "bool" => {
            if evaled.len() != 1 {
                return Err("bool() expects 1 arg".into());
            }
            Ok(Value::Boolean(evaled[0].is_truthy()))
        }
        "type" => {
            if evaled.len() != 1 {
                return Err("type() expects 1 arg".into());
            }
            Ok(Value::String(evaled[0].type_name().into()))
        }
        "input" => {
            let mut l = String::new();
            std::io::stdin()
                .read_line(&mut l)
                .map_err(|e| e.to_string())?;
            Ok(Value::String(l.trim().into()))
        }
        "abs" => {
            if evaled.len() != 1 {
                return Err("abs() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.abs()))
                .ok_or_else(|| "abs() expects number".into())
        }
        "sin" => {
            if evaled.len() != 1 {
                return Err("sin() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.sin()))
                .ok_or_else(|| "sin() expects number".into())
        }
        "cos" => {
            if evaled.len() != 1 {
                return Err("cos() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.cos()))
                .ok_or_else(|| "cos() expects number".into())
        }
        "sqrt" => {
            if evaled.len() != 1 {
                return Err("sqrt() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.sqrt()))
                .ok_or_else(|| "sqrt() expects number".into())
        }
        "say" => {
            for v in &evaled {
                print!("{}", v);
            }
            Ok(Value::Nil)
        }
        "shout" => {
            for v in &evaled {
                print!("{}", v);
            }
            println!();
            Ok(Value::Nil)
        }
        "read" => {
            if evaled.len() != 1 {
                return Err("read() expects 1 arg".into());
            }
            let path = match &evaled[0] {
                Value::String(s) => s.clone(),
                v => return Err(format!("read() expects string path, got {}", v)),
            };
            fs::read_to_string(Path::new(&path))
                .map(Value::String)
                .map_err(|e| format!("Could not read '{}': {}", path, e))
        }
        "write" => {
            if evaled.len() != 2 {
                return Err("write() expects 2 args".into());
            }
            let path = match &evaled[0] {
                Value::String(s) => s.clone(),
                v => return Err(format!("write() expects string path, got {}", v)),
            };
            let content = match &evaled[1] {
                Value::String(s) => s.clone(),
                v => return Err(format!("write() expects string content, got {}", v)),
            };
            fs::write(Path::new(&path), &content)
                .map_err(|e| format!("Could not write '{}': {}", path, e))?;
            Ok(Value::Nil)
        }
        "lines" => {
            if evaled.len() != 1 {
                return Err("lines() expects 1 arg".into());
            }
            let path = match &evaled[0] {
                Value::String(s) => s.clone(),
                v => return Err(format!("lines() expects string, got {}", v)),
            };
            let content = fs::read_to_string(Path::new(&path))
                .map_err(|e| format!("Could not read '{}': {}", path, e))?;
            Ok(Value::List(gc.alloc(GcData::List(
                content.lines().map(|l| Value::String(l.into())).collect(),
            ))))
        }
        "assert" => {
            if evaled.is_empty() || evaled.len() > 2 {
                return Err("assert() expects 1 or 2 args".into());
            }
            if !evaled[0].is_truthy() {
                let msg = if evaled.len() == 2 {
                    match &evaled[1] {
                        Value::String(s) => s.clone(),
                        _ => evaled[1].to_string(),
                    }
                } else {
                    "assertion failed".into()
                };
                return Err(msg);
            }
            Ok(Value::Nil)
        }
        "split" => {
            if evaled.len() != 2 {
                return Err("split() expects 2 args".into());
            }
            let (s, sep) = (evaled[0].to_string(), evaled[1].to_string());
            Ok(Value::List(gc.alloc(GcData::List(
                s.split(&sep)
                    .map(|p| Value::String(p.into()))
                    .collect(),
            ))))
        }
        "trim" => {
            if evaled.len() != 1 {
                return Err("trim() expects 1 arg".into());
            }
            Ok(Value::String(evaled[0].to_string().trim().into()))
        }
        "upper" => {
            if evaled.len() != 1 {
                return Err("upper() expects 1 arg".into());
            }
            Ok(Value::String(evaled[0].to_string().to_uppercase()))
        }
        "lower" => {
            if evaled.len() != 1 {
                return Err("lower() expects 1 arg".into());
            }
            Ok(Value::String(evaled[0].to_string().to_lowercase()))
        }
        "contains" => {
            if evaled.len() != 2 {
                return Err("contains() expects 2 args".into());
            }
            let haystack = evaled[0].to_string();
            let needle = evaled[1].to_string();
            Ok(Value::Boolean(haystack.contains(&needle)))
        }
        "replace" => {
            if evaled.len() != 3 {
                return Err("replace() expects 3 args".into());
            }
            let s = evaled[0].to_string();
            let from = evaled[1].to_string();
            let to = evaled[2].to_string();
            Ok(Value::String(s.replace(&from, &to)))
        }
        "push" => {
            if evaled.len() != 2 {
                return Err("push() expects 2 args".into());
            }
            let h = match &evaled[0] {
                Value::List(h) => *h,
                v => return Err(format!("push() expects list, got {}", v.type_name())),
            };
            if let GcData::List(items) = gc.get_mut(h) {
                items.push(evaled[1].clone());
            }
            Ok(Value::List(h))
        }
        "pop" => {
            if evaled.len() != 1 {
                return Err("pop() expects 1 arg".into());
            }
            let h = match evaled.into_iter().next() {
                Some(Value::List(h)) => h,
                Some(v) => {
                    return Err(format!("pop() expects list, got {}", v.type_name()))
                }
                None => unreachable!(),
            };
            if let GcData::List(items) = gc.get_mut(h) {
                if items.is_empty() {
                    return Err("pop() on empty list".into());
                }
                Ok(items.pop().unwrap())
            } else {
                unreachable!()
            }
        }
        "sort" => {
            if evaled.len() != 1 {
                return Err("sort() expects 1 arg".into());
            }
            let h = match evaled.into_iter().next() {
                Some(Value::List(h)) => h,
                Some(v) => return Err(format!("sort() expects list, got {}", v.type_name())),
                None => unreachable!(),
            };
            if let GcData::List(items) = gc.get_mut(h) {
                items.sort_by_key(|a| a.to_string());
            }
            Ok(Value::List(h))
        }
        "reverse" => {
            if evaled.len() != 1 {
                return Err("reverse() expects 1 arg".into());
            }
            let h = match evaled.into_iter().next() {
                Some(Value::List(h)) => h,
                Some(v) => return Err(format!("reverse() expects list, got {}", v.type_name())),
                None => unreachable!(),
            };
            if let GcData::List(items) = gc.get_mut(h) {
                items.reverse();
            }
            Ok(Value::List(h))
        }
        "join" => {
            if evaled.len() != 2 {
                return Err("join() expects 2 args".into());
            }
            let sep = evaled[1].to_string();
            match &evaled[0] {
                Value::List(h) => {
                    if let GcData::List(items) = gc.get(*h) {
                        let strs: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                        Ok(Value::String(strs.join(&sep)))
                    } else {
                        unreachable!()
                    }
                }
                v => Err(format!("join() expects list, got {}", v.type_name())),
            }
        }
        "floor" => {
            if evaled.len() != 1 {
                return Err("floor() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.floor()))
                .ok_or_else(|| "floor() expects number".into())
        }
        "ceil" => {
            if evaled.len() != 1 {
                return Err("ceil() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.ceil()))
                .ok_or_else(|| "ceil() expects number".into())
        }
        "round" => {
            if evaled.len() != 1 {
                return Err("round() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.round()))
                .ok_or_else(|| "round() expects number".into())
        }
        "max" => {
            if evaled.len() != 2 {
                return Err("max() expects 2 args".into());
            }
            match (evaled[0].as_float(), evaled[1].as_float()) {
                (Some(a), Some(b)) => Ok(Value::Float(a.max(b))),
                _ => Err("max() expects numbers".into()),
            }
        }
        "min" => {
            if evaled.len() != 2 {
                return Err("min() expects 2 args".into());
            }
            match (evaled[0].as_float(), evaled[1].as_float()) {
                (Some(a), Some(b)) => Ok(Value::Float(a.min(b))),
                _ => Err("min() expects numbers".into()),
            }
        }
        "pow" => {
            if evaled.len() != 2 {
                return Err("pow() expects 2 args".into());
            }
            match (evaled[0].as_float(), evaled[1].as_float()) {
                (Some(a), Some(b)) => Ok(Value::Float(a.powf(b))),
                _ => Err("pow() expects numbers".into()),
            }
        }
        "log" => {
            if evaled.len() != 1 {
                return Err("log() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.ln()))
                .ok_or_else(|| "log() expects number".into())
        }
        "exp" => {
            if evaled.len() != 1 {
                return Err("exp() expects 1 arg".into());
            }
            evaled[0]
                .as_float()
                .map(|n| Value::Float(n.exp()))
                .ok_or_else(|| "exp() expects number".into())
        }
        "json_encode" => {
            if evaled.len() != 1 {
                return Err("json_encode() expects 1 arg".into());
            }
            Ok(Value::String(json_stringify(&evaled[0], gc)))
        }
        "json_decode" => {
            if evaled.len() != 1 {
                return Err("json_decode() expects 1 arg".into());
            }
            let s = match &evaled[0] {
                Value::String(s) => s.clone(),
                v => {
                    return Err(format!(
                        "json_decode() expects string, got {}",
                        v.type_name()
                    ))
                }
            };
            json_parse(&s, gc)
        }
        "json_validate" => {
            if evaled.len() != 1 {
                return Err("json_validate() expects 1 arg".into());
            }
            let s = match &evaled[0] {
                Value::String(s) => s.as_str(),
                v => {
                    return Err(format!(
                        "json_validate() expects string, got {}",
                        v.type_name()
                    ))
                }
            };
            Ok(Value::Boolean(json_validate(s, gc)))
        }
        "clock" => {
            let dur = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?;
            Ok(Value::Float(dur.as_secs_f64()))
        }
        "exit" => {
            let code = if evaled.len() == 1 {
                evaled[0].as_float().map(|n| n as i32).unwrap_or(0)
            } else {
                0
            };
            std::process::exit(code);
        }
        "panic" => {
            if evaled.len() != 1 {
                return Err("panic() expects 1 arg".into());
            }
            let msg = match &evaled[0] {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            Err(msg)
        }
        "unreachable" => {
            if !evaled.is_empty() {
                return Err("unreachable() expects 0 args".into());
            }
            Err("entered unreachable code".into())
        }
        "set" => {
            if evaled.is_empty() {
                return Ok(Value::Set(gc.alloc(GcData::Set(Vec::new()))));
            }
            if evaled.len() == 1 {
                match &evaled[0] {
                    Value::List(h) => {
                        let items = if let GcData::List(items) = gc.get(*h) {
                            items.clone()
                        } else {
                            unreachable!()
                        };
                        let mut seen = Vec::new();
                        let mut result = Vec::new();
                        for item in items.iter() {
                            if !seen.contains(item) {
                                seen.push(item.clone());
                                result.push(item.clone());
                            }
                        }
                        return Ok(Value::Set(gc.alloc(GcData::Set(result))));
                    }
                    v => return Err(format!("set() expects list or no args, got {}", v.type_name())),
                }
            }
            Err("set() expects 0 or 1 argument".into())
        }
        "Ok" => {
            if evaled.len() != 1 {
                return Err("Ok() expects 1 arg".into());
            }
            Ok(Value::Ok(Box::new(evaled.into_iter().next().unwrap())))
        }
        "Err" => {
            if evaled.len() != 1 {
                return Err("Err() expects 1 arg".into());
            }
            Ok(Value::Err(Box::new(evaled.into_iter().next().unwrap())))
        }
        "Some" => {
            if evaled.len() != 1 {
                return Err("Some() expects 1 arg".into());
            }
            Ok(Value::Some(Box::new(evaled.into_iter().next().unwrap())))
        }
        _ => Err(format!("Unknown builtin '{}'", name)),
    }
}
