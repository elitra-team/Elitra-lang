use std::collections::HashMap;
use std::fmt;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Condvar, Mutex};

use crate::ast::{ClassMethod, Expr, Stmt, Type};

use super::gc::{GcData, GcHandle, GcHeap, GC_HEAP};

type EnumVariants = Vec<(String, Vec<(String, Option<Type>)>)>;

#[derive(Debug)]
pub struct FutureState {
    pub mutex: Mutex<Option<Result<Value, String>>>,
    pub cvar: Condvar,
}

impl FutureState {
    pub fn new() -> Self {
        FutureState {
            mutex: Mutex::new(None),
            cvar: Condvar::new(),
        }
    }
}

pub type SharedFuture = Arc<FutureState>;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Nil,
    Ok(Box<Value>),
    Err(Box<Value>),
    Some(Box<Value>),
    List(GcHandle),
    Set(GcHandle),
    Dict(GcHandle),
    Range(f64, f64),
    Tuple(Vec<Value>),
    Function {
        name: String,
        #[allow(dead_code)]
        generic_params: Vec<String>,
        params: Vec<(String, Option<Type>, Option<Expr>)>,
        #[allow(dead_code)]
        return_type: Option<Type>,
        body: Arc<Vec<Stmt>>,
        is_async: bool,
    },
    Closure {
        #[allow(dead_code)]
        generic_params: Vec<String>,
        params: Vec<(String, Option<Type>, Option<Expr>)>,
        body: Arc<Vec<Stmt>>,
        captured: GcHandle,
        is_async: bool,
    },
    BuiltinFn(String),
    Module(HashMap<String, Value>),
    StructDef { name: String, fields: Vec<(String, Option<Type>)> },
    StructInstance { name: String, fields: GcHandle },
    EnumDef { name: String, variants: EnumVariants },
    Iterator(IterKind),
    Future(SharedFuture),
    SuperRef(String),
    TraitObject {
        trait_name: String,
        value: Box<Value>,
        methods: Vec<(String, Value)>,
    },
    ClassDef {
        name: String,
        parent: Option<String>,
        methods: Vec<ClassMethod>,
    },
    ClassInstance {
        class_name: String,
        fields: GcHandle,
    },
}

#[derive(Debug)]
pub enum IterKind {
    List { handle: GcHandle, index: usize },
    String { chars: Vec<String>, index: usize },
    Range { start: f64, end: f64, current: f64 },
    Generator { rx: Arc<Mutex<Receiver<Value>>>, exhausted: bool },
    Map { inner: Box<IterKind>, func: Box<Value> },
    Filter { inner: Box<IterKind>, func: Box<Value> },
    Take { inner: Box<IterKind>, remaining: usize },
    Skip { inner: Box<IterKind>, remaining: usize },
    Enumerate { inner: Box<IterKind>, index: usize },
    Zip { inner1: Box<IterKind>, inner2: Box<IterKind> },
    Chain { inner: Box<IterKind>, next: Option<Box<IterKind>> },
    Flatten { inner: Box<IterKind>, current_sub: Option<Box<IterKind>> },
}

impl Clone for IterKind {
    fn clone(&self) -> Self {
        match self {
            IterKind::List { handle, index } => IterKind::List { handle: *handle, index: *index },
            IterKind::String { chars, index } => IterKind::String { chars: chars.clone(), index: *index },
            IterKind::Range { start, end, current } => IterKind::Range { start: *start, end: *end, current: *current },
            IterKind::Generator { rx, exhausted } => IterKind::Generator { rx: rx.clone(), exhausted: *exhausted },
            IterKind::Map { inner, func } => IterKind::Map { inner: inner.clone(), func: func.clone() },
            IterKind::Filter { inner, func } => IterKind::Filter { inner: inner.clone(), func: func.clone() },
            IterKind::Take { inner, remaining } => IterKind::Take { inner: inner.clone(), remaining: *remaining },
            IterKind::Skip { inner, remaining } => IterKind::Skip { inner: inner.clone(), remaining: *remaining },
            IterKind::Enumerate { inner, index } => IterKind::Enumerate { inner: inner.clone(), index: *index },
            IterKind::Zip { inner1, inner2 } => IterKind::Zip { inner1: inner1.clone(), inner2: inner2.clone() },
            IterKind::Chain { inner, next } => IterKind::Chain { inner: inner.clone(), next: next.clone() },
            IterKind::Flatten { inner, current_sub } => IterKind::Flatten { inner: inner.clone(), current_sub: current_sub.clone() },
        }
    }
}

impl PartialEq for IterKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (IterKind::List { handle: a, index: ai }, IterKind::List { handle: b, index: bi }) => a == b && ai == bi,
            (IterKind::String { chars: a, index: ai }, IterKind::String { chars: b, index: bi }) => a == b && ai == bi,
            (IterKind::Range { start: as_, end: ae, current: ac }, IterKind::Range { start: bs, end: be, current: bc }) => {
                (as_ - bs).abs() < f64::EPSILON && (ae - be).abs() < f64::EPSILON && (ac - bc).abs() < f64::EPSILON
            }
            (IterKind::Generator { exhausted: a, .. }, IterKind::Generator { exhausted: b, .. }) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum Flow {
    None,
    Return(Value),
    Break,
    Continue,
}

fn trace_value_handles(v: &Value, out: &mut Vec<GcHandle>) {
    match v {
        Value::List(h) | Value::Set(h) | Value::Dict(h) => out.push(*h),
        Value::StructInstance { fields: h, .. } | Value::ClassInstance { fields: h, .. } => out.push(*h),
        Value::Closure { captured: h, .. } => out.push(*h),
        Value::Ok(inner) | Value::Err(inner) | Value::Some(inner) => trace_value_handles(inner, out),
        Value::Iterator(iter) => trace_iter_kind_handles(iter, out),
        Value::TraitObject { value, methods, .. } => {
            trace_value_handles(value, out);
            for (_, v) in methods {
                trace_value_handles(v, out);
            }
        }
        Value::Tuple(items) => {
            for item in items {
                trace_value_handles(item, out);
            }
        }
        _ => {}
    }
}

pub fn trace_iter_kind_handles(iter: &IterKind, out: &mut Vec<GcHandle>) {
    match iter {
        IterKind::List { handle: h, .. } => out.push(*h),
        IterKind::Map { inner, func } => {
            trace_iter_kind_handles(inner, out);
            trace_value_handles(func, out);
        }
        IterKind::Filter { inner, func } => {
            trace_iter_kind_handles(inner, out);
            trace_value_handles(func, out);
        }
        IterKind::Take { inner, .. } | IterKind::Skip { inner, .. } | IterKind::Enumerate { inner, .. } => {
            trace_iter_kind_handles(inner, out);
        }
        IterKind::Zip { inner1, inner2 } => {
            trace_iter_kind_handles(inner1, out);
            trace_iter_kind_handles(inner2, out);
        }
        IterKind::Chain { inner, next } => {
            trace_iter_kind_handles(inner, out);
            if let Some(n) = next {
                trace_iter_kind_handles(n, out);
            }
        }
        IterKind::Flatten { inner, current_sub } => {
            trace_iter_kind_handles(inner, out);
            if let Some(sub) = current_sub {
                trace_iter_kind_handles(sub, out);
            }
        }
        _ => {}
    }
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Boolean(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(h) | Value::Set(h) | Value::Dict(h) => {
                GC_HEAP.with(|cell| {
                    if let Some(ptr) = cell.get() {
                        let gc = unsafe { &*ptr };
                        match gc.get(*h) {
                            GcData::List(v) | GcData::Set(v) => !v.is_empty(),
                            GcData::Dict(v) => !v.is_empty(),
                            _ => true,
                        }
                    } else {
                        true
                    }
                })
            }
            Value::Tuple(v) => !v.is_empty(),
            _ => true,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Float(n) => Some(*n),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Boolean(_) => "bool",
            Value::Nil => "none",
            Value::Ok(_) => "ok",
            Value::Err(_) => "err",
            Value::Some(_) => "some",
            Value::List(_) => "list",
            Value::Set(_) => "set",
            Value::Dict(_) => "dict",
            Value::Range(_, _) => "range",
            Value::Tuple(_) => "tuple",
            Value::Function { .. } | Value::Closure { .. } | Value::BuiltinFn(_) => "~",
            Value::Module(_) => "module",
            Value::StructDef { .. } => "struct",
            Value::StructInstance { .. } => "struct",
            Value::EnumDef { .. } => "enum",
            Value::Iterator(_) => "iterator",
            Value::Future(_) => "future",
            Value::ClassDef { .. } => "class",
            Value::ClassInstance { .. } => "class",
            Value::SuperRef(_) => "super",
            Value::TraitObject { .. } => "trait_obj",
        }
    }

    pub fn eq_deep(&self, other: &Value, heap: &GcHeap) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => *a as f64 == *b,
            (Value::Float(a), Value::Int(b)) => *a == *b as f64,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::List(a), Value::List(b)) => {
                let ad = heap.get(*a);
                let bd = heap.get(*b);
                match (ad, bd) {
                    (GcData::List(av), GcData::List(bv)) => {
                        av.len() == bv.len() && av.iter().zip(bv.iter()).all(|(x, y)| x.eq_deep(y, heap))
                    }
                    _ => false,
                }
            }
            (Value::Set(a), Value::Set(b)) => {
                let ad = heap.get(*a);
                let bd = heap.get(*b);
                match (ad, bd) {
                    (GcData::Set(av), GcData::Set(bv)) => {
                        av.len() == bv.len() && av.iter().all(|x| bv.iter().any(|y| x.eq_deep(y, heap)))
                    }
                    _ => false,
                }
            }
            (Value::Dict(a), Value::Dict(b)) => {
                let ad = heap.get(*a);
                let bd = heap.get(*b);
                match (ad, bd) {
                    (GcData::Dict(av), GcData::Dict(bv)) => {
                        av.len() == bv.len() && av.iter().all(|(ak, av)| {
                            bv.iter().any(|(bk, bv)| ak.eq_deep(bk, heap) && av.eq_deep(bv, heap))
                        })
                    }
                    _ => false,
                }
            }
        (Value::Range(a1, a2), Value::Range(b1, b2)) => a1 == b1 && a2 == b2,
            (Value::Tuple(a), Value::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.eq_deep(y, heap))
            }
            (Value::Function { name: n1, params: p1, body: b1, is_async: a1, .. },
             Value::Function { name: n2, params: p2, body: b2, is_async: a2, .. }) => {
                n1 == n2 && p1 == p2 && b1 == b2 && a1 == a2
            }
            (Value::Closure { params: p1, body: b1, captured: c1, is_async: a1, .. },
             Value::Closure { params: p2, body: b2, captured: c2, is_async: a2, .. }) => {
                if p1 != p2 || b1 != b2 || a1 != a2 { return false; }
                let cd1 = heap.get(*c1);
                let cd2 = heap.get(*c2);
                match (cd1, cd2) {
                    (GcData::Captured(s1), GcData::Captured(s2)) => {
                        s1.len() == s2.len() && s1.iter().zip(s2.iter()).all(|(m1, m2)| {
                            m1.len() == m2.len() && m1.iter().all(|(k, v)| {
                                m2.get(k).map_or(false, |v2| v.eq_deep(v2, heap))
                            })
                        })
                    }
                    _ => false,
                }
            }
            (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
            (Value::StructDef { name: n1, fields: f1 }, Value::StructDef { name: n2, fields: f2 }) => {
                n1 == n2 && f1 == f2
            }
            (Value::StructInstance { name: n1, fields: f1 }, Value::StructInstance { name: n2, fields: f2 }) => {
                if n1 != n2 { return false; }
                let fd1 = heap.get(*f1);
                let fd2 = heap.get(*f2);
                match (fd1, fd2) {
                    (GcData::StructFields(m1), GcData::StructFields(m2)) => {
                        m1.len() == m2.len() && m1.iter().all(|(k, v)| {
                            m2.get(k).map_or(false, |v2| v.eq_deep(v2, heap))
                        })
                    }
                    _ => false,
                }
            }
            (Value::EnumDef { name: n1, variants: v1 }, Value::EnumDef { name: n2, variants: v2 }) => {
                n1 == n2 && v1 == v2
            }
            (Value::Iterator(a), Value::Iterator(b)) => a == b,
            (Value::Module(a), Value::Module(b)) => a == b,
            (Value::Future(_), _) | (_, Value::Future(_)) => false,
            (Value::TraitObject { trait_name: tn1, value: v1, .. },
             Value::TraitObject { trait_name: tn2, value: v2, .. }) => tn1 == tn2 && v1.eq_deep(v2, heap),
            (Value::ClassDef { name: n1, parent: p1, .. }, Value::ClassDef { name: n2, parent: p2, .. }) => n1 == n2 && p1 == p2,
            (Value::ClassInstance { class_name: n1, fields: f1 }, Value::ClassInstance { class_name: n2, fields: f2 }) => {
                if n1 != n2 { return false; }
                let fd1 = heap.get(*f1);
                let fd2 = heap.get(*f2);
                match (fd1, fd2) {
                    (GcData::ClassFields(m1), GcData::ClassFields(m2)) => {
                        m1.len() == m2.len() && m1.iter().all(|(k, v)| {
                            m2.get(k).map_or(false, |v2| v.eq_deep(v2, heap))
                        })
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    pub fn collect_gc_handles(&self, out: &mut Vec<GcHandle>) {
        trace_value_handles(self, out);
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        GC_HEAP.with(|cell| {
            if let Some(ptr) = cell.get() {
                let heap = unsafe { &*ptr };
                self.eq_deep(other, heap)
            } else {
                shallow_eq(self, other)
            }
        })
    }
}

fn shallow_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::Int(a), Value::Float(b)) => *a as f64 == *b,
        (Value::Float(a), Value::Int(b)) => *a == *b as f64,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Boolean(a), Value::Boolean(b)) => a == b,
        (Value::Nil, Value::Nil) => true,
        (Value::List(a), Value::List(b)) => a == b,
        (Value::Set(a), Value::Set(b)) => a == b,
        (Value::Dict(a), Value::Dict(b)) => a == b,
        (Value::Range(a1, a2), Value::Range(b1, b2)) => a1 == b1 && a2 == b2,
        (Value::Function { name: n1, params: p1, body: b1, is_async: a1, .. },
         Value::Function { name: n2, params: p2, body: b2, is_async: a2, .. }) => {
            n1 == n2 && p1 == p2 && b1 == b2 && a1 == a2
        }
        (Value::Closure { params: p1, body: b1, captured: c1, is_async: a1, .. },
         Value::Closure { params: p2, body: b2, captured: c2, is_async: a2, .. }) => {
            p1 == p2 && b1 == b2 && c1 == c2 && a1 == a2
        }
        (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
        (Value::StructDef { name: n1, fields: f1 }, Value::StructDef { name: n2, fields: f2 }) => {
            n1 == n2 && f1 == f2
        }
        (Value::StructInstance { name: n1, fields: f1 }, Value::StructInstance { name: n2, fields: f2 }) => {
            n1 == n2 && f1 == f2
        }
        (Value::EnumDef { name: n1, variants: v1 }, Value::EnumDef { name: n2, variants: v2 }) => {
            n1 == n2 && v1 == v2
        }
        (Value::Iterator(a), Value::Iterator(b)) => a == b,
        (Value::Module(a), Value::Module(b)) => a == b,
        (Value::Future(_), _) | (_, Value::Future(_)) => false,
        (Value::TraitObject { trait_name: tn1, value: v1, .. },
         Value::TraitObject { trait_name: tn2, value: v2, .. }) => tn1 == tn2 && v1 == v2,
        (Value::ClassDef { name: n1, parent: p1, .. }, Value::ClassDef { name: n2, parent: p2, .. }) => n1 == n2 && p1 == p2,
        (Value::ClassInstance { class_name: n1, fields: f1 }, Value::ClassInstance { class_name: n2, fields: f2 }) => {
            n1 == n2 && f1 == f2
        }
        _ => false,
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        GC_HEAP.with(|cell| {
            let heap = cell.get().map(|ptr| unsafe { &*ptr });
            match self {
                Value::Int(n) => write!(f, "{}", n),
                Value::Float(n) => {
                    if n.fract() == 0.0 && n.is_finite() {
                        write!(f, "{}.0", *n as i64)
                    } else {
                        write!(f, "{}", n)
                    }
                }
                Value::String(s) => write!(f, "{}", s),
                Value::Boolean(b) => write!(f, "{}", b),
                Value::Nil => write!(f, "none"),
                Value::Ok(v) => write!(f, "Ok({})", v),
                Value::Err(v) => write!(f, "Err({})", v),
                Value::Some(v) => write!(f, "Some({})", v),
                Value::List(h) => {
                    if let Some(gc) = heap {
                        if let GcData::List(items) = gc.get(*h) {
                            let strs: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                            return write!(f, "[{}]", strs.join(", "));
                        }
                    }
                    write!(f, "[<gc {}>]", h.0)
                }
                Value::Set(h) => {
                    if let Some(gc) = heap {
                        if let GcData::Set(items) = gc.get(*h) {
                            let strs: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                            return write!(f, "{{{}}}", strs.join(", "));
                        }
                    }
                    write!(f, "{{<gc {}>}}", h.0)
                }
                Value::Dict(h) => {
                    if let Some(gc) = heap {
                        if let GcData::Dict(pairs) = gc.get(*h) {
                            let strs: Vec<String> = pairs.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                            return write!(f, "{{{}}}", strs.join(", "));
                        }
                    }
                    write!(f, "{{<gc {}>}}", h.0)
                }
                Value::Range(a, b) => write!(f, "{}..{}", a, b),
                Value::Tuple(items) => {
                    let strs: Vec<String> = items.iter().map(|v| v.to_string()).collect();
                    write!(f, "({})", strs.join(", "))
                }
                Value::Function { name, params, generic_params, .. } => {
                    let ps: Vec<String> = params.iter().map(|(n, _, _)| n.clone()).collect();
                    let gs = if generic_params.is_empty() { String::new() } else { format!("<{}>", generic_params.join(", ")) };
                    write!(f, "<~{}{}>({})", name, gs, ps.join(", "))
                }
                Value::Closure { params, .. } => {
                    let ps: Vec<String> = params.iter().map(|(n, _, _)| n.clone()).collect();
                    write!(f, "<closure>({})", ps.join(", "))
                }
                Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
                Value::Module(_) => write!(f, "<module>"),
                Value::StructDef { name, .. } => write!(f, "<struct {}>", name),
                Value::StructInstance { name, fields: h } => {
                    if let Some(gc) = heap {
                        if let GcData::StructFields(fields) = gc.get(*h) {
                            let strs: Vec<String> = fields.iter()
                                .map(|(k, v)| format!("{}: {}", k, v)).collect();
                            return write!(f, "{}({})", name, strs.join(", "));
                        }
                    }
                    write!(f, "{}<gc>", name)
                }
                Value::EnumDef { name, .. } => write!(f, "<enum {}>", name),
                Value::Future(_) => write!(f, "<future>"),
                Value::ClassDef { name, .. } => write!(f, "<class {}>", name),
                Value::ClassInstance { class_name, fields: h } => {
                    if let Some(gc) = heap {
                        if let GcData::ClassFields(fields) = gc.get(*h) {
                            let strs: Vec<String> = fields.iter()
                                .map(|(k, v)| format!("{}: {}", k, v)).collect();
                            return write!(f, "{}({})", class_name, strs.join(", "));
                        }
                    }
                    write!(f, "{}<gc>", class_name)
                }
                Value::SuperRef(name) => write!(f, "<super of {}>", name),
                Value::TraitObject { trait_name, value, .. } => {
                    write!(f, "dyn {}: {}", trait_name, value.as_ref())
                }
                Value::Iterator(kind) => match kind {
                    IterKind::List { handle, index } => {
                        if let Some(gc) = heap {
                            if let GcData::List(items) = gc.get(*handle) {
                                if *index < items.len() {
                                    return write!(f, "<iter: {} left>", items.len() - index);
                                }
                            }
                        }
                        write!(f, "<iter>")
                    }
                    IterKind::String { chars, index } => {
                        if *index < chars.len() {
                            write!(f, "<iter: {} chars left>", chars.len() - index)
                        } else {
                            write!(f, "<iter: exhausted>")
                        }
                    }
                    IterKind::Generator { exhausted, .. } => {
                        if *exhausted {
                            write!(f, "<generator: exhausted>")
                        } else {
                            write!(f, "<generator>")
                        }
                    }
                    IterKind::Range { start, end, current } => {
                        write!(f, "<iter: {}..{} @{}>", start, end, current)
                    }
                    IterKind::Map { .. } => write!(f, "<iter: map>"),
                    IterKind::Filter { .. } => write!(f, "<iter: filter>"),
                    IterKind::Take { remaining, .. } => write!(f, "<iter: take({})>", remaining),
                    IterKind::Skip { remaining, .. } => write!(f, "<iter: skip({})>", remaining),
                    IterKind::Enumerate { .. } => write!(f, "<iter: enumerate>"),
                    IterKind::Zip { .. } => write!(f, "<iter: zip>"),
                    IterKind::Chain { .. } => write!(f, "<iter: chain>"),
                    IterKind::Flatten { .. } => write!(f, "<iter: flatten>"),
                },
            }
        })
    }
}
