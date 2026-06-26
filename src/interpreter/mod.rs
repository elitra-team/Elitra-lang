mod builtins;
pub(crate) mod debugger;
pub(crate) mod gc;
pub(crate) mod json;
mod macro_expander;
pub mod value;
mod stdlib;

pub use value::{Value, IterKind};

use self::debugger::Debugger;
use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io::{self, BufRead, Write};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use crate::ast::{BinaryOpKind, ClassMethod, CompClause, DestructureItem, DestructureTarget, Expr, MatchArm, MatchPattern, SourceSpan, Stmt, TraitMethod, TraitMethodImpl, Type, UnaryOpKind};
use crate::lexer::Lexer;
use crate::package::PackageManager;
use crate::parser::Parser;


use self::builtins::{call_builtin, register_builtins};
use self::gc::{GcData, GcHandle, GcHeap};
use self::stdlib::{call_stdlib, get_all as get_stdlib_modules};
use self::value::{Flow, FutureState, SharedFuture};

use crate::jit::JitEngine;

thread_local! {
    pub(crate) static JIT_INTERP: Cell<Option<*mut Interpreter>> = const { Cell::new(None) };
}

pub struct Interpreter {
    globals: HashMap<String, Value>,
    locals: Vec<HashMap<String, Value>>,
    pub type_check: bool,
    pub jit_enabled: bool,
    loop_depth: usize,
    std_modules: HashMap<String, Value>,
    package: PackageManager,
    current_file: Option<String>,
    pub jit: Option<JitEngine>,
    call_stack: Vec<(String, Option<SourceSpan>)>,
    current_span: Option<SourceSpan>,
    pub(crate) generator_channel: Option<SyncSender<Value>>,
    pub gc: GcHeap,
    pub debugger: Option<Debugger>,
    current_class: Option<String>,
    trait_defs: HashMap<String, Vec<TraitMethod>>,
    trait_impls: HashMap<String, HashMap<String, Vec<(String, Value)>>>,
    thrown_value: Option<Value>,
}

fn contains_yield(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match s {
            Stmt::Yield { .. } => return true,
            Stmt::If { then_branch, else_branch, .. } => {
                if contains_yield(then_branch) { return true; }
                if let Some(b) = else_branch {
                    if contains_yield(b) { return true; }
                }
            }
            Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::DoWhile { body, .. } => {
                if contains_yield(body) { return true; }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    if contains_yield(&arm.body) { return true; }
                }
            }
            Stmt::Try { body, catch_body, .. } => {
                if contains_yield(body) { return true; }
                if contains_yield(catch_body) { return true; }
            }
            _ => {}
        }
    }
    false
}

impl Interpreter {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        register_builtins(&mut globals);
        let std_modules = get_stdlib_modules();
        let mut package = PackageManager::new();
        for (name, module_val) in &std_modules {
            if let Value::Module(bindings) = module_val {
                let mut lines = Vec::new();
                for k in bindings.keys() {
                    lines.push(format!("{} = nil", k));
                }
                let source = lines.join("\n");
                package.register_std_module(name, &source);
            }
        }
        Interpreter {
            globals,
            locals: Vec::new(),
            type_check: false,
            jit_enabled: false,
            loop_depth: 0,
            std_modules,
            package,
            current_file: None,
            jit: None,
            call_stack: Vec::new(),
            current_span: None,
            generator_channel: None,
            gc: GcHeap::new(),
            debugger: None,
            current_class: None,
            trait_defs: HashMap::new(),
            trait_impls: HashMap::new(),
            thrown_value: None,
        }
    }

    pub fn gc_list(&mut self, items: Vec<Value>) -> Value {
        Value::List(self.gc.alloc(GcData::List(items)))
    }

    pub fn gc_set(&mut self, items: Vec<Value>) -> Value {
        Value::Set(self.gc.alloc(GcData::Set(items)))
    }

    pub fn gc_dict(&mut self, pairs: Vec<(Value, Value)>) -> Value {
        Value::Dict(self.gc.alloc(GcData::Dict(pairs)))
    }

    pub fn gc_struct(&mut self, name: String, fields: HashMap<String, Value>) -> Value {
        Value::StructInstance { name, fields: self.gc.alloc(GcData::StructFields(fields)) }
    }

    pub fn gc_class(&mut self, class_name: String, fields: HashMap<String, Value>) -> Value {
        Value::ClassInstance { class_name, fields: self.gc.alloc(GcData::ClassFields(fields)) }
    }

    pub fn gc_captured(&mut self, captured: Vec<HashMap<String, Value>>) -> GcHandle {
        self.gc.alloc(GcData::Captured(captured))
    }

    pub fn get_list_data(&self, h: GcHandle) -> &Vec<Value> {
        match self.gc.get(h) { GcData::List(v) => v, _ => unreachable!("not a list") }
    }

    #[allow(dead_code)]
    pub fn get_list_data_mut(&mut self, h: GcHandle) -> &mut Vec<Value> {
        match self.gc.get_mut(h) { GcData::List(v) => v, _ => unreachable!("not a list") }
    }

    pub fn get_set_data(&self, h: GcHandle) -> &Vec<Value> {
        match self.gc.get(h) { GcData::Set(v) => v, _ => unreachable!("not a set") }
    }

    #[allow(dead_code)]
    pub fn get_set_data_mut(&mut self, h: GcHandle) -> &mut Vec<Value> {
        match self.gc.get_mut(h) { GcData::Set(v) => v, _ => unreachable!("not a set") }
    }

    pub fn get_dict_data(&self, h: GcHandle) -> &Vec<(Value, Value)> {
        match self.gc.get(h) { GcData::Dict(v) => v, _ => unreachable!("not a dict") }
    }

    #[allow(dead_code)]
    pub fn get_dict_data_mut(&mut self, h: GcHandle) -> &mut Vec<(Value, Value)> {
        match self.gc.get_mut(h) { GcData::Dict(v) => v, _ => unreachable!("not a dict") }
    }

    pub fn get_struct_fields(&self, h: GcHandle) -> &HashMap<String, Value> {
        match self.gc.get(h) { GcData::StructFields(m) => m, _ => unreachable!("not a struct") }
    }

    pub fn get_struct_fields_mut(&mut self, h: GcHandle) -> &mut HashMap<String, Value> {
        match self.gc.get_mut(h) { GcData::StructFields(m) => m, _ => unreachable!("not a struct") }
    }

    pub fn get_class_fields(&self, h: GcHandle) -> &HashMap<String, Value> {
        match self.gc.get(h) { GcData::ClassFields(m) => m, _ => unreachable!("not a class instance") }
    }

    pub fn get_class_fields_mut(&mut self, h: GcHandle) -> &mut HashMap<String, Value> {
        match self.gc.get_mut(h) { GcData::ClassFields(m) => m, _ => unreachable!("not a class instance") }
    }

    pub fn get_captured_data(&self, h: GcHandle) -> &Vec<HashMap<String, Value>> {
        match self.gc.get(h) { GcData::Captured(v) => v, _ => unreachable!("not captured") }
    }

    #[allow(dead_code)]
    pub fn get_captured_data_mut(&mut self, h: GcHandle) -> &mut Vec<HashMap<String, Value>> {
        match self.gc.get_mut(h) { GcData::Captured(v) => v, _ => unreachable!("not captured") }
    }

    pub fn call_global_fn(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        let func = self.get(name)?;
        self.call_func_with_values(name, func, args)
    }

    pub fn jit_call(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        let func = self.get(name)?;
        match &func {
            Value::Function { params, .. } | Value::Closure { params, .. } => {
                let has_defaults = params.iter().any(|(_, _, d)| d.is_some());
                if args.len() > params.len() || (!has_defaults && args.len() != params.len()) {
                    let msg = if has_defaults {
                        format!("'{}' expects at most {} args, got {}", name, params.len(), args.len())
                    } else {
                        format!("'{}' expects {} args, got {}", name, params.len(), args.len())
                    };
                    return Err(msg);
                }
                self.call_func_with_values(name, func, args)
            }
            Value::BuiltinFn(bn) => {
                match bn.as_str() {
                    "map" | "filter" | "fold" | "take" | "collect" | "iter" | "as_trait" => {
                        self.call_special_builtin(bn, args)
                    }
                    _ => call_builtin(bn, args, &mut self.gc),
                }
            }
            _ => Err(format!("'{}' is not a function", name)),
        }
    }

    fn push_call(&mut self, name: &str) {
        self.call_stack.push((name.to_string(), self.current_span.clone()));
    }

    fn pop_call(&mut self) {
        self.call_stack.pop();
    }

    fn trace_err(&self, msg: &str) -> String {
        let file = self.current_file.as_deref().unwrap_or("<unknown>");
        let mut out = String::new();
        // Build stack trace from outer to inner
        let frames: Vec<_> = self.call_stack.iter().rev().collect();
        if frames.is_empty() {
            if let Some(span) = &self.current_span {
                return format!("{}:{}: {}", file, span.line, msg);
            }
            return msg.to_string();
        }
        for (func_name, span_opt) in &frames {
            let loc = span_opt.as_ref().map(|s| format!("{}:{}", file, s.line)).unwrap_or_else(|| "?".to_string());
            out.push_str(&format!("  in '{}' ({})\n", func_name, loc));
        }
        if let Some(span) = &self.current_span {
            out.push_str(&format!("  at {}:{}\n", file, span.line));
        }
        out.push_str(&format!("error: {}", msg));
        out
    }

    fn runtime_err(&self, msg: impl Into<String>) -> String {
        self.trace_err(&msg.into())
    }

    pub fn jit_method_call(&mut self, method: &str, obj: Value, all_args: Vec<Value>) -> Result<Value, String> {
        match &obj {
            Value::Module(bindings) => {
                if let Some(val) = bindings.get(method) {
                    match val {
                        Value::BuiltinFn(_) | Value::Function { .. } | Value::Closure { .. } => {
                            return self.call_func_with_values(method, val.clone(), all_args[1..].to_vec());
                        }
                        _ => {
                            return Ok(val.clone());
                        }
                    }
                }
                Err(format!("Module has no '{}'", method))
            }
            Value::TraitObject { trait_name, value, methods } => {
                let inner_val = (**value).clone();
                if let Some((_, func)) = methods.iter().find(|(n, _)| n == method) {
                    let func_name = match func {
                        Value::Function { name, .. } => name.clone(),
                        _ => method.to_string(),
                    };
                    let mut args = vec![inner_val];
                    args.extend(all_args[1..].iter().cloned());
                    return self.call_func_with_values(&func_name, func.clone(), args);
                }
                let concrete_type = match value.as_ref() {
                    Value::StructInstance { name, .. } => name.clone(),
                    Value::ClassInstance { class_name, .. } => class_name.clone(),
                    v => v.type_name().to_string(),
                };
                if let Some(method_func) = self.trait_impls
                    .get(trait_name)
                    .and_then(|impls| impls.get(&concrete_type))
                    .and_then(|methods| methods.iter().find(|(n, _)| n == method))
                    .map(|(_, f)| f.clone())
                {
                    let func_name = match &method_func {
                        Value::Function { name, .. } => name.clone(),
                        _ => method.to_string(),
                    };
                    let mut args = vec![inner_val];
                    args.extend(all_args[1..].iter().cloned());
                    return self.call_func_with_values(&func_name, method_func, args);
                }
                Err(format!("Method '{}' not found for trait '{}' on type '{}'", method, trait_name, concrete_type))
            }
            Value::ClassInstance { class_name, .. } => {
                let cname = class_name.clone();
                let class_val = self.get(&cname)?;
                if let Value::ClassDef { methods, .. } = &class_val {
                    if let Some(cm) = methods.iter().find(|m| m.name == *method) {
                        let orig_obj = all_args[0].clone();
                        self.locals.push(HashMap::new());
                        let all_args_clone = all_args.clone();
                        for ((pn, _, _), av) in cm.params.iter().zip(all_args_clone) {
                            self.define(pn, av);
                        }
                        let old_class = self.current_class.clone();
                        self.current_class = Some(cname.clone());
                        let mut result = Value::Nil;
                        let last_is_expr = matches!(cm.body.last(), Some(Stmt::Expr { .. }));
                        for (i, s) in cm.body.iter().enumerate() {
                            if i + 1 == cm.body.len() && last_is_expr {
                                if let Stmt::Expr { span: _, expr } = s {
                                    result = self.eval(expr)?;
                                }
                                break;
                            }
                            match self.exec(s)? {
                                Flow::None => {}
                                Flow::Return(v) => { result = v; break; }
                                Flow::Break => return Err(self.trace_err("break in method")),
                                Flow::Continue => return Err(self.trace_err("continue in method")),
                            }
                        }
                        self.current_class = old_class;
                        if let Some(self_val) = self.locals.last().and_then(|l| l.get("self")) {
                            if let Value::ClassInstance { fields: init_h, .. } = self_val {
                                if let Value::ClassInstance { fields: obj_h, .. } = &orig_obj {
                                    let init_fields = self.get_class_fields(*init_h).clone();
                                    let obj_fields = self.get_class_fields_mut(*obj_h);
                                    *obj_fields = init_fields;
                                }
                            }
                        }
                        self.locals.pop();
                        return Ok(result);
                    }
                    match self.find_method_in_parents(&cname, method) {
                        Some(cm) => {
                            let orig_obj = all_args[0].clone();
                            self.locals.push(HashMap::new());
                            let all_args_clone = all_args.clone();
                            for ((pn, _, _), av) in cm.params.iter().zip(all_args_clone) {
                                self.define(pn, av);
                            }
                            let old_class = self.current_class.clone();
                            self.current_class = Some(cname.clone());
                            let mut result = Value::Nil;
                            let last_is_expr = matches!(cm.body.last(), Some(Stmt::Expr { .. }));
                            for (i, s) in cm.body.iter().enumerate() {
                                if i + 1 == cm.body.len() && last_is_expr {
                                    if let Stmt::Expr { span: _, expr } = s {
                                        result = self.eval(expr)?;
                                    }
                                    break;
                                }
                                match self.exec(s)? {
                                    Flow::None => {}
                                    Flow::Return(v) => { result = v; break; }
                                    Flow::Break => return Err(self.trace_err("break in method")),
                                    Flow::Continue => return Err(self.trace_err("continue in method")),
                                }
                            }
                            self.current_class = old_class;
                            if let Some(self_val) = self.locals.last().and_then(|l| l.get("self")) {
                                if let Value::ClassInstance { fields: init_h, .. } = self_val {
                                    if let Value::ClassInstance { fields: obj_h, .. } = &orig_obj {
                                        let init_fields = self.get_class_fields(*init_h).clone();
                                        let obj_fields = self.get_class_fields_mut(*obj_h);
                                        *obj_fields = init_fields;
                                    }
                                }
                            }
                            self.locals.pop();
                            return Ok(result);
                        }
                        None => {}
                    }
                }
                Err(format!("Class '{}' has no method '{}'", cname, method))
            }
            Value::Iterator(kind) => {
                let mut owned_kind = (*kind).clone();
                match method {
                    "next" => Err("jit: iter.next not supported in JIT".into()),
                    "map" => {
                        let func = all_args.get(1)
                            .ok_or_else(|| "map() needs a function argument".to_string())?
                            .clone();
                        Ok(Value::Iterator(IterKind::Map { inner: Box::new(owned_kind), func: Box::new(func) }))
                    }
                    "filter" => {
                        let func = all_args.get(1)
                            .ok_or_else(|| "filter() needs a function argument".to_string())?
                            .clone();
                        Ok(Value::Iterator(IterKind::Filter { inner: Box::new(owned_kind), func: Box::new(func) }))
                    }
                    "take" => {
                        let n_arg = all_args.get(1)
                            .ok_or_else(|| "take() needs a number argument".to_string())?;
                        let n = n_arg.as_float()
                            .ok_or_else(|| format!("take() expects number, got {}", n_arg.type_name()))? as usize;
                        Ok(Value::Iterator(IterKind::Take { inner: Box::new(owned_kind), remaining: n }))
                    }
                    "skip" => {
                        let n_arg = all_args.get(1)
                            .ok_or_else(|| "skip() needs a number argument".to_string())?;
                        let n = n_arg.as_float()
                            .ok_or_else(|| format!("skip() expects number, got {}", n_arg.type_name()))? as usize;
                        Ok(Value::Iterator(IterKind::Skip { inner: Box::new(owned_kind), remaining: n }))
                    }
                    "enumerate" => {
                        Ok(Value::Iterator(IterKind::Enumerate { inner: Box::new(owned_kind), index: 0 }))
                    }
                    "zip" => {
                        let other = all_args.get(1)
                            .ok_or_else(|| "zip() needs an iterable argument".to_string())?
                            .clone();
                        let other_iter = self.make_iter(&other)?;
                        if let Value::Iterator(other_kind) = other_iter {
                            Ok(Value::Iterator(IterKind::Zip {
                                inner1: Box::new(owned_kind),
                                inner2: Box::new(other_kind),
                            }))
                        } else {
                            Err("zip(): failed to create iterator".to_string())
                        }
                    }
                    "chain" => {
                        let other = all_args.get(1)
                            .ok_or_else(|| "chain() needs an iterable argument".to_string())?
                            .clone();
                        let other_iter = self.make_iter(&other)?;
                        if let Value::Iterator(other_kind) = other_iter {
                            Ok(Value::Iterator(IterKind::Chain {
                                inner: Box::new(owned_kind),
                                next: Some(Box::new(other_kind)),
                            }))
                        } else {
                            Err("chain(): failed to create iterator".to_string())
                        }
                    }
                    "flatten" => {
                        Ok(Value::Iterator(IterKind::Flatten {
                            inner: Box::new(owned_kind),
                            current_sub: None,
                        }))
                    }
                    "collect" => {
                        let mut items = Vec::new();
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => break,
                                val => items.push(val),
                            }
                        }
                        Ok(self.gc_list(items))
                    }
                    "fold" => {
                        let acc = all_args.get(1)
                            .ok_or_else(|| "fold() needs an initial value".to_string())?
                            .clone();
                        let func = all_args.get(2)
                            .ok_or_else(|| "fold() needs a function".to_string())?
                            .clone();
                        let mut result = acc;
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => break,
                                val => {
                                    result = self.call_func_with_values("fold callback", func.clone(), vec![result, val])?;
                                }
                            }
                        }
                        Ok(result)
                    }
                    "for_each" => {
                        let func = all_args.get(1)
                            .ok_or_else(|| "for_each() needs a function".to_string())?
                            .clone();
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => break,
                                val => {
                                    self.call_func_with_values("for_each callback", func.clone(), vec![val])?;
                                }
                            }
                        }
                        Ok(Value::Nil)
                    }
                    "all" => {
                        let func = all_args.get(1)
                            .ok_or_else(|| "all() needs a function".to_string())?
                            .clone();
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => return Ok(Value::Boolean(true)),
                                val => {
                                    let ok = self.call_func_with_values("all callback", func.clone(), vec![val])?;
                                    if !ok.is_truthy() {
                                        return Ok(Value::Boolean(false));
                                    }
                                }
                            }
                        }
                    }
                    "any" => {
                        let func = all_args.get(1)
                            .ok_or_else(|| "any() needs a function".to_string())?
                            .clone();
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => return Ok(Value::Boolean(false)),
                                val => {
                                    let ok = self.call_func_with_values("any callback", func.clone(), vec![val])?;
                                    if ok.is_truthy() {
                                        return Ok(Value::Boolean(true));
                                    }
                                }
                            }
                        }
                    }
                    "count" => {
                        let mut c = 0i64;
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => return Ok(Value::Int(c)),
                                _ => c += 1,
                            }
                        }
                    }
                    "nth" => {
                        let n = all_args.get(1)
                            .ok_or_else(|| "nth() needs a number argument".to_string())?;
                        let target = n.as_float().ok_or_else(|| format!("nth() expects number, got {}", n.type_name()))? as usize;
                        let mut skip_count = target;
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => return Ok(Value::Nil),
                                val => {
                                    if skip_count == 0 {
                                        return Ok(val);
                                    }
                                    skip_count -= 1;
                                }
                            }
                        }
                    }
                    "last" => {
                        let mut last = Value::Nil;
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => return Ok(last),
                                val => last = val,
                            }
                        }
                    }
                    "sum" => {
                        let mut total = Value::Int(0);
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => break,
                                val => {
                                    total = self.eval_binary(total, &BinaryOpKind::Add, val)?;
                                }
                            }
                        }
                        Ok(total)
                    }
                    "min" => {
                        let mut first = true;
                        let mut min_val = Value::Nil;
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => {
                                    if first { return Ok(Value::Nil); }
                                    return Ok(min_val);
                                }
                                val => {
                                    if first || self.eval_binary(val.clone(), &BinaryOpKind::Less, min_val.clone())?.is_truthy() {
                                        min_val = val;
                                    }
                                    first = false;
                                }
                            }
                        }
                    }
                    "max" => {
                        let mut first = true;
                        let mut max_val = Value::Nil;
                        loop {
                            let next = self.iter_next(&mut owned_kind);
                            match next {
                                Value::Nil => {
                                    if first { return Ok(Value::Nil); }
                                    return Ok(max_val);
                                }
                                val => {
                                    if first || self.eval_binary(val.clone(), &BinaryOpKind::Greater, max_val.clone())?.is_truthy() {
                                        max_val = val;
                                    }
                                    first = false;
                                }
                            }
                        }
                    }
                    _ => Err(format!("Iterator has no method '{}'", method)),
                }
            }
            _ => {
                if method == "iter" {
                    return self.make_iter(&obj).map(|v| v);
                }
                let func = self.get(method)?;
                self.call_func_with_values(method, func, all_args)
            }
        }
    }

    pub fn set_current_file(&mut self, path: &str) {
        self.current_file = Some(path.to_string());
    }

    fn collect_roots(&mut self) {
        let mut roots = Vec::new();
        for (_, v) in &self.globals {
            v.collect_gc_handles(&mut roots);
        }
        for scope in &self.locals {
            for (_, v) in scope {
                v.collect_gc_handles(&mut roots);
            }
        }
        for (_, inner) in &self.trait_impls {
            for (_, methods) in inner {
                for (_, v) in methods {
                    v.collect_gc_handles(&mut roots);
                }
            }
        }
        for (_, v) in &self.std_modules {
            v.collect_gc_handles(&mut roots);
        }
        if !roots.is_empty() {
            self.gc.collect(&roots);
        }
    }

    fn debug_before_stmt(&mut self, stmt: &Stmt) {
        let file = self.current_file.as_deref().unwrap_or("<unknown>");
        if let Some(ref mut dbg) = self.debugger {
            if dbg.before_stmt(stmt, file) {
                self.debug_repl();
            }
        }
    }

    fn debug_before_call(&mut self) {
        if let Some(ref mut dbg) = self.debugger {
            dbg.before_call();
        }
    }

    fn debug_after_call(&mut self) {
        if let Some(ref mut dbg) = self.debugger {
            dbg.after_call();
        }
    }

    fn debug_show_source(&self, file: &str, line: usize) {
        if let Ok(content) = std::fs::read_to_string(file) {
            let start = line.saturating_sub(3);
            let lines: Vec<&str> = content.lines().collect();
            for i in start.saturating_sub(1)..lines.len().min(start + 6) {
                let marker = if i + 1 == line { "=>" } else { "  " };
                println!("{} {:4}: {}", marker, i + 1, lines[i]);
            }
        }
    }

    fn debug_show_backtrace(&self) {
        if self.call_stack.is_empty() {
            return;
        }
        println!("  Call stack:");
        for (i, (name, span)) in self.call_stack.iter().rev().enumerate() {
            let loc = span.as_ref().map(|s| format!(":{}", s.line)).unwrap_or_default();
            println!("    #{} {}{}", i, name, loc);
        }
    }

    fn debug_repl(&mut self) {
        let file = self.current_file.as_deref().unwrap_or("<unknown>").to_string();
        let line = self.current_span.as_ref().map(|s| s.line).unwrap_or(0);

        println!("\n[DEBUG] at {}:{}", file, line);
        self.debug_show_source(&file, line);
        self.debug_show_backtrace();

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            print!("(dbg) ");
            let _ = stdout.flush();

            let mut input = String::new();
            if stdin.lock().read_line(&mut input).is_err() {
                break;
            }
            let input = input.trim();
            if input.is_empty() {
                continue;
            }

            let (cmd, arg) = input.split_once(char::is_whitespace)
                .map(|(c, a)| (c, Some(a.trim())))
                .unwrap_or((input, None));

            match cmd {
                "c" | "continue" => {
                    if let Some(ref mut dbg) = self.debugger {
                        dbg.mode = self::debugger::DebugMode::Running;
                    }
                    return;
                }
                "n" | "next" => {
                    if let Some(ref mut dbg) = self.debugger {
                        dbg.step_depth = dbg.call_depth;
                        dbg.mode = self::debugger::DebugMode::StepOver;
                    }
                    return;
                }
                "s" | "step" => {
                    if let Some(ref mut dbg) = self.debugger {
                        dbg.mode = self::debugger::DebugMode::StepInto;
                    }
                    return;
                }
                "f" | "finish" => {
                    if let Some(ref mut dbg) = self.debugger {
                        dbg.step_depth = dbg.call_depth;
                        dbg.mode = self::debugger::DebugMode::StepOut;
                    }
                    return;
                }
                "p" | "print" => {
                    if let Some(expr) = arg {
                        match self.eval_debug_expr(expr) {
                            Ok(val) => println!("{}", val),
                            Err(e) => println!("Error: {}", e),
                        }
                    }
                }
                "l" | "list" => {
                    let file = self.current_file.as_deref().unwrap_or("<unknown>");
                    let line = self.current_span.as_ref().map(|s| s.line).unwrap_or(0);
                    self.debug_show_source(file, line);
                }
                "bt" | "backtrace" => {
                    self.debug_show_backtrace();
                }
                "b" | "break" => {
                    if let Some(loc) = arg {
                        if let Ok(line) = loc.parse::<usize>() {
                            if let Some(ref mut dbg) = self.debugger {
                                let num = dbg.breakpoints.len() + 1;
                                dbg.breakpoints.push(self::debugger::Breakpoint {
                                    file: file.clone(),
                                    line,
                                    enabled: true,
                                });
                                println!("Breakpoint {} at line {}", num, line);
                            }
                        } else {
                            println!("Usage: b <line_number>");
                        }
                    }
                }
                "d" | "delete" => {
                    if let Some(num_str) = arg.and_then(|s| s.parse::<usize>().ok()) {
                        if let Some(ref mut dbg) = self.debugger {
                            if num_str > 0 && num_str <= dbg.breakpoints.len() {
                                let bp = dbg.breakpoints.remove(num_str - 1);
                                println!("Deleted breakpoint at line {}", bp.line);
                            }
                        }
                    }
                }
                "i" | "info" => {
                    if let Some(ref dbg) = self.debugger {
                        if dbg.breakpoints.is_empty() {
                            println!("No breakpoints");
                        } else {
                            for (i, bp) in dbg.breakpoints.iter().enumerate() {
                                println!("  {}: {}:{}", i + 1, bp.file, bp.line);
                            }
                        }
                    }
                }
                "v" | "vars" => {
                    for scope in self.locals.iter().rev() {
                        for (name, val) in scope {
                            println!("  {} = {}", name, val);
                        }
                    }
                    for (name, val) in &self.globals {
                        println!("  {} = {}", name, val);
                    }
                }
                "h" | "help" => {
                    println!("Debugger commands:");
                    println!("  c, continue     Resume execution");
                    println!("  n, next         Step over");
                    println!("  s, step         Step into");
                    println!("  f, finish       Step out");
                    println!("  p, print <expr> Evaluate expression");
                    println!("  l, list         Show source around current line");
                    println!("  bt, backtrace   Show call stack");
                    println!("  b, break <line> Set breakpoint");
                    println!("  d, delete <n>   Delete breakpoint");
                    println!("  i, info         Show breakpoints");
                    println!("  v, vars         Show local and global variables");
                    println!("  h, help         Show this help");
                    println!("  q, quit         Exit");
                }
                "q" | "quit" => {
                    std::process::exit(0);
                }
                _ => {
                    println!("Unknown command: '{}'. Type 'h' for help.", cmd);
                }
            }
        }
    }

    pub fn eval_debug_expr(&mut self, source: &str) -> Result<Value, String> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize();
        let mut parser = Parser::new(tokens);
        let stmts = parser.parse().map_err(|e| format!("Parse error: {}", e))?;
        match stmts.into_iter().next() {
            Some(Stmt::Expr { expr, .. }) => self.eval(&expr),
            Some(Stmt::Let { value, .. }) => self.eval(&value),
            _ => Err("Cannot evaluate expression".into()),
        }
    }

    fn expand_macros(&self, stmts: &[Stmt]) -> Result<Vec<Stmt>, String> {
        use self::macro_expander::MacroExpander;
        let mut expander = MacroExpander::new();
        for stmt in stmts {
            if let Stmt::Macro { name, params, body, .. } = stmt {
                let clean_params: Vec<String> = params.iter()
                    .map(|p| p.strip_prefix('$').unwrap_or(p).to_string())
                    .collect();
                expander.register(name.clone(), clean_params, body.clone());
            }
        }
        if !expander.has_macros() {
            return Ok(stmts.to_vec());
        }
        expander.expand(stmts)
    }

    pub fn interpret(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        let _guard = self::gc::GcGuard::set(&mut self.gc);
        let stmts = self.expand_macros(stmts)?;
        for (i, s) in stmts.iter().enumerate() {
            self.current_span = s.span();
            match self.exec(s).map_err(|e| self.trace_err(&e))? {
                Flow::None => {}
                Flow::Return(_) => return Err(self.trace_err("return outside function")),
                Flow::Break => return Err(self.trace_err("break outside loop")),
                Flow::Continue => return Err(self.trace_err("continue outside loop")),
            }
            if i % 100 == 0 && i > 0 {
                self.collect_roots();
            }
        }
        Ok(())
    }

    fn get(&self, name: &str) -> Result<Value, String> {
        for s in self.locals.iter().rev() {
            if let Some(v) = s.get(name) {
                return Ok(v.clone());
            }
        }
        self.globals
            .get(name)
            .cloned()
            .ok_or_else(|| format!("Undefined '{}'", name))
    }

    fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        for s in self.locals.iter_mut().rev() {
            if s.contains_key(name) {
                s.insert(name.into(), value);
                return Ok(());
            }
        }
        if self.globals.contains_key(name) {
            self.globals.insert(name.into(), value);
            return Ok(());
        }
        Err(format!("Undefined '{}'", name))
    }

    fn define(&mut self, name: &str, value: Value) {
        if self.locals.is_empty() {
            self.globals.insert(name.into(), value);
        } else {
            self.locals.last_mut().unwrap().insert(name.into(), value);
        }
    }

    fn bind_destructure(&mut self, target: &DestructureTarget, val: Value) -> Result<Flow, String> {
        match target {
            DestructureTarget::List(items) => {
                let list = match val {
                    Value::List(h) => self.get_list_data(h).clone(),
                    _ => return Err(format!("Cannot destructure '{}' as list", val.type_name())),
                };
                let mut idx = 0;
                for item in items {
                    match item {
                        DestructureItem::Name(name) => {
                            let v = list.get(idx).cloned()
                                .ok_or_else(|| format!("Destructure index {} out of bounds", idx))?;
                            self.define(name, v);
                            idx += 1;
                        }
                        DestructureItem::Rest(name) => {
                            let rest: Vec<Value> = list[idx..].to_vec();
                            let rest_val = self.gc_list(rest);
                            self.define(name, rest_val);
                            idx = list.len();
                        }
                    }
                }
                Ok(Flow::None)
            }
            DestructureTarget::Struct(fields) => {
                // Pre-collect all field values to avoid borrow conflicts
                let field_vals: Vec<(String, Value)> = {
                    let mut out = Vec::new();
                    for field in fields {
                        let v = match &val {
                            Value::Dict(h) => {
                                let pairs = self.get_dict_data(*h);
                                pairs.iter()
                                    .find(|(k, _)| match k { Value::String(s) => s == field, _ => false })
                                    .map(|(_, v)| v.clone())
                                    .ok_or_else(|| format!("Field '{}' not found in dict", field))?
                            }
                            Value::StructInstance { fields: h, .. } => {
                                let fmap = self.get_struct_fields(*h);
                                fmap.get(field).cloned()
                                    .ok_or_else(|| format!("Field '{}' not found", field))?
                            }
                            Value::ClassInstance { fields: h, .. } => {
                                let fmap = self.get_class_fields(*h);
                                fmap.get(field).cloned()
                                    .ok_or_else(|| format!("Field '{}' not found", field))?
                            }
                            _ => return Err(format!("Cannot destructure '{}' as struct/dict", val.type_name())),
                        };
                        out.push((field.clone(), v));
                    }
                    out
                };
                for (field, v) in field_vals {
                    self.define(&field, v);
                }
                Ok(Flow::None)
            }
        }
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<Flow, String> {
        self.current_span = stmt.span();
        self.debug_before_stmt(stmt);
        match stmt {
            Stmt::Let {
                span: _,
                pub_flag: _,
                name,
                type_ann,
                value,
            } => {
                let val = self.eval(value)?;
                if self.type_check && let Some(t) = type_ann && !self.check_type(&val, t) {
                    return Err(format!(
                        "Type error: expected '{}', got '{}'",
                        t,
                        val.type_name()
                    ));
                }
                self.define(name, val);
                Ok(Flow::None)
            }
            Stmt::Struct { span: _, pub_flag: _, name, fields } => {
                self.define(name, Value::StructDef { name: name.clone(), fields: fields.clone() });
                Ok(Flow::None)
            }
            Stmt::Enum { span: _, pub_flag: _, name, variants } => {
                type EnumVariants = Vec<(String, Vec<(String, Option<Type>)>)>;
                let vs: EnumVariants = variants.iter()
                    .map(|v| (v.name.clone(), v.fields.clone())).collect();
                self.define(name, Value::EnumDef { name: name.clone(), variants: vs });
                Ok(Flow::None)
            }
            Stmt::Class { span: _, pub_flag: _, name, extends, methods } => {
                self.define(name, Value::ClassDef { name: name.clone(), parent: extends.clone(), methods: methods.clone() });
                Ok(Flow::None)
            }
            Stmt::Fn {
                span: _,
                pub_flag: _,
                name,
                generic_params,
                params,
                return_type,
                body,
                is_async,
            } => {
                self.define(
                    name,
                    Value::Function {
                        name: name.clone(),
                        generic_params: generic_params.clone(),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        body: Arc::new(body.clone()),
                        is_async: *is_async,
                    },
                );
                Ok(Flow::None)
            }
            Stmt::If {
                span: _,
                condition,
                then_branch,
                else_branch,
            } => {
                let branch = if self.eval(condition)?.is_truthy() {
                    then_branch
                } else if let Some(eb) = else_branch {
                    eb
                } else {
                    return Ok(Flow::None);
                };
                for s in branch {
                    match self.exec(s)? {
                        Flow::None => {}
                        f => return Ok(f),
                    }
                }
                Ok(Flow::None)
            }
            Stmt::While { span: _, condition, body } => {
                self.loop_depth += 1;
                loop {
                    if !self.eval(condition)?.is_truthy() {
                        break;
                    }
                    for s in body {
                        match self.exec(s)? {
                            Flow::None => {}
                            Flow::Break => {
                                self.loop_depth -= 1;
                                return Ok(Flow::None);
                            }
                            Flow::Continue => break,
                            f => {
                                self.loop_depth -= 1;
                                return Ok(f);
                            }
                        }
                    }
                }
                self.loop_depth -= 1;
                Ok(Flow::None)
            }
            Stmt::DoWhile { span: _, condition, body } => {
                self.loop_depth += 1;
                loop {
                    for s in body {
                        match self.exec(s)? {
                            Flow::None => {}
                            Flow::Break => {
                                self.loop_depth -= 1;
                                return Ok(Flow::None);
                            }
                            Flow::Continue => break,
                            f => {
                                self.loop_depth -= 1;
                                return Ok(f);
                            }
                        }
                    }
                    if !self.eval(condition)?.is_truthy() {
                        break;
                    }
                }
                self.loop_depth -= 1;
                Ok(Flow::None)
            }
            Stmt::Destructure { span: _, pub_flag: _, target, value } => {
                let val = self.eval(value)?;
                self.bind_destructure(target, val)
            }
            Stmt::Yield { span: _, value } => {
                let v = self.eval(value)?;
                if let Some(ref tx) = self.generator_channel {
                    tx.send(v).map_err(|_| "generator receiver closed".to_string())?;
                    Ok(Flow::None)
                } else {
                    Err("'yield' outside generator".into())
                }
            }
            Stmt::Throw { span: _, value } => {
                let v = self.eval(value)?;
                self.thrown_value = Some(v.clone());
                Err(format!("{}", v))
            }
            Stmt::Macro { .. } => Ok(Flow::None),
            Stmt::Trait { name, methods, .. } => {
                self.trait_defs.insert(name.clone(), methods.clone());
                Ok(Flow::None)
            }
            Stmt::Impl { span: _, trait_name, type_name, methods } => {
                // Collect provided method names
                let provided: std::collections::HashSet<String> = methods.iter().map(|m| m.name.clone()).collect();
                // Fill missing methods from trait defaults
                let mut all_methods = methods.clone();
                if let Some(trait_methods) = self.trait_defs.get(trait_name) {
                    for tm in trait_methods {
                        if !provided.contains(&tm.name) {
                            if let Some(body) = &tm.body {
                                all_methods.push(TraitMethodImpl {
                                    name: tm.name.clone(),
                                    params: tm.params.clone(),
                                    return_type: tm.return_type.clone(),
                                    body: body.clone(),
                                });
                            }
                        }
                    }
                }
                // Store impl methods as `type_method` functions callable by name
                let mut method_funcs = Vec::new();
                for m in &all_methods {
                    let fname = format!("{}_{}", type_name, m.name);
                    let func = Value::Function {
                        name: fname.clone(),
                        generic_params: Vec::new(),
                        params: m.params.clone(),
                        return_type: m.return_type.clone(),
                        body: Arc::new(m.body.clone()),
                        is_async: false,
                    };
                    self.define(&fname, func.clone());
                    method_funcs.push((m.name.clone(), func));
                }
                // Register in trait impl registry for runtime dispatch
                self.trait_impls
                    .entry(trait_name.clone())
                    .or_default()
                    .insert(type_name.clone(), method_funcs);
                Ok(Flow::None)
            }
            Stmt::For {
                span: _,
                var,
                iterable,
                body,
            } => {
                let iter = self.eval(iterable)?;
                self.loop_depth += 1;
                let result = match iter {
                    Value::Range(s, e) => {
                        for i in (s as i64)..(e as i64) {
                            self.locals.push(HashMap::new());
                            self.define(var, Value::Int(i));
                            let f = self.run_loop(body);
                            self.locals.pop();
                            match f {
                                Flow::None => {}
                                Flow::Break => break,
                                Flow::Continue => continue,
                                f => {
                                    return Ok(f);
                                }
                            }
                        }
                        Ok(Flow::None)
                    }
                    Value::List(h) => {
                        let items = self.get_list_data(h).clone();
                        for item in items {
                            self.locals.push(HashMap::new());
                            self.define(var, item);
                            let f = self.run_loop(body);
                            self.locals.pop();
                            match f {
                                Flow::None => {}
                                Flow::Break => break,
                                Flow::Continue => continue,
                                f => {
                                    return Ok(f);
                                }
                            }
                        }
                        Ok(Flow::None)
                    }
                    Value::String(s) => {
                        for ch in s.chars() {
                            self.locals.push(HashMap::new());
                            self.define(var, Value::String(ch.to_string()));
                            let f = self.run_loop(body);
                            self.locals.pop();
                            match f {
                                Flow::None => {}
                                Flow::Break => break,
                                Flow::Continue => continue,
                                f => {
                                    return Ok(f);
                                }
                            }
                        }
                        Ok(Flow::None)
                    }
                    Value::Iterator(kind) => {
                        let result = match kind {
                            IterKind::List { handle: h, .. } => {
                                let items = self.get_list_data(h).clone();
                                for item in items {
                                    self.locals.push(HashMap::new());
                                    self.define(var, item);
                                    let f = self.run_loop(body);
                                    self.locals.pop();
                                    match f {
                                        Flow::None => {}
                                        Flow::Break => break,
                                        Flow::Continue => continue,
                                        f => { return Ok(f); }
                                    }
                                }
                                Ok(Flow::None)
                            }
                            IterKind::String { chars, .. } => {
                                let chs = chars.clone();
                                for ch in chs {
                                    self.locals.push(HashMap::new());
                                    self.define(var, Value::String(ch));
                                    let f = self.run_loop(body);
                                    self.locals.pop();
                                    match f {
                                        Flow::None => {}
                                        Flow::Break => break,
                                        Flow::Continue => continue,
                                        f => { return Ok(f); }
                                    }
                                }
                                Ok(Flow::None)
                            }
                            IterKind::Generator { rx, .. } => {
                                loop {
                                    match rx.lock().unwrap().recv() {
                                        Ok(val) => {
                                            self.locals.push(HashMap::new());
                                            self.define(var, val);
                                            let f = self.run_loop(body);
                                            self.locals.pop();
                                            match f {
                                                Flow::None => {}
                                                Flow::Break => break,
                                                Flow::Continue => continue,
                                                f => { return Ok(f); }
                                            }
                                        }
                                        Err(_) => break,
                                    }
                                }
                                Ok(Flow::None)
                            }
                            IterKind::Range { start: _start, end, .. } => {
                                for i in (_start as i64)..(end as i64) {
                                    self.locals.push(HashMap::new());
                                    self.define(var, Value::Int(i));
                                    let f = self.run_loop(body);
                                    self.locals.pop();
                                    match f {
                                        Flow::None => {}
                                        Flow::Break => break,
                                        Flow::Continue => continue,
                                        f => { return Ok(f); }
                                    }
                                }
                                Ok(Flow::None)
                            }
                            mut other => {
                                loop {
                                    let next = self.iter_next(&mut other);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            self.locals.push(HashMap::new());
                                            self.define(var, val);
                                            let f = self.run_loop(body);
                                            self.locals.pop();
                                            match f {
                                                Flow::None => {}
                                                Flow::Break => break,
                                                Flow::Continue => continue,
                                                f => { return Ok(f); }
                                            }
                                        }
                                    }
                                }
                                Ok(Flow::None)
                            }
                        };
                        self.loop_depth -= 1;
                        return result;
                    }
                    iter_val => {
                        let mut iter_val = match self.make_iter(&iter_val) {
                            Ok(v @ Value::Iterator(_)) => v,
                            _ => return Err("Cannot iterate".into()),
                        };
                        loop {
                            match &mut iter_val {
                                Value::Iterator(kind) => {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            self.locals.push(HashMap::new());
                                            self.define(var, val);
                                            let f = self.run_loop(body);
                                            self.locals.pop();
                                            match f {
                                                Flow::None => {}
                                                Flow::Break => break,
                                                Flow::Continue => {}
                                                f => { return Ok(f); }
                                            }
                                        }
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        self.loop_depth -= 1;
                        return Ok(Flow::None);
                    }
                };
                self.loop_depth -= 1;
                result
            }
            Stmt::Break { .. } => {
                if self.loop_depth == 0 {
                    return Err("break outside loop".into());
                }
                Ok(Flow::Break)
            }
            Stmt::Continue { .. } => {
                if self.loop_depth == 0 {
                    return Err("continue outside loop".into());
                }
                Ok(Flow::Continue)
            }
            Stmt::Match { span: _, value, arms } => {
                let val = self.eval(value)?;
                for arm in arms {
                    let mut bindings = HashMap::new();
                    let matched = self.match_pattern(&arm.pattern, &val, &mut bindings)?;
                    if matched {
                        if let Some(guard) = &arm.guard {
                            self.locals.push(HashMap::new());
                            for (k, v) in &bindings {
                                self.define(k, v.clone());
                            }
                            let guard_val = self.eval(guard)?;
                            self.locals.pop();
                            if !guard_val.is_truthy() { continue; }
                        }
                        self.locals.push(HashMap::new());
                        for (k, v) in &bindings {
                            self.define(k, v.clone());
                        }
                        for s in &arm.body {
                            match self.exec(s)? {
                                Flow::None => {}
                                f => {
                                    self.locals.pop();
                                    return Ok(f);
                                }
                            }
                        }
                        self.locals.pop();
                        break;
                    }
                }
                Ok(Flow::None)
            }
            Stmt::Try {
                span: _,
                body,
                catch_var,
                catch_body,
            } => {
                let ok = self.exec_block(body);
                match ok {
                    Ok(Flow::None) => Ok(Flow::None),
                    Ok(Flow::Return(v)) => Ok(Flow::Return(v)),
                    Ok(Flow::Break) => Ok(Flow::Break),
                    Ok(Flow::Continue) => Ok(Flow::Continue),
                    Err(e) => {
                        self.locals.push(HashMap::new());
                        let catch_val = self.thrown_value.take().unwrap_or_else(|| Value::String(e));
                        self.define(catch_var, catch_val);
                        let result = self.exec_block(catch_body);
                        self.locals.pop();
                        result.map(|_| Flow::None)
                    }
                }
            }
            Stmt::Return { span: _, value } => Ok(Flow::Return(self.eval(value)?)),
            Stmt::Print { span: _, value, newline } => {
                let val = self.eval(value)?;
                if *newline {
                    println!("{}", val);
                } else {
                    print!("{}", val);
                }
                Ok(Flow::None)
            }
            Stmt::Import { span: _, pub_flag: _, path, alias } => {
                if path.starts_with("std/") {
                    let module_name = path.strip_prefix("std/").unwrap_or("");
                    let module_val = self.std_modules.get(module_name)
                        .ok_or_else(|| format!("Unknown std module '{}'", module_name))?
                        .clone();
                    if let Some(a) = alias {
                        self.define(a, module_val);
                    } else {
                        if let Value::Module(bindings) = &module_val {
                            for (k, v) in bindings {
                                self.define(k, v.clone());
                            }
                        }
                    }
                    return Ok(Flow::None);
                }
                let resolved = self.package.resolve(path, self.current_file.as_deref())
                    .map_err(|e| format!("{}", e))?;
                let source = self.package.load_source(&resolved)
                    .map_err(|e| format!("{}", e))?;
                if let Some(a) = alias {
                    let module = self.exec_import_module_source(&source)?;
                    self.define(a, module);
                } else {
                    self.exec_import_flat_source(&source)?;
                }
                Ok(Flow::None)
            }
            Stmt::Expr { span: _, expr } => {
                self.eval(expr)?;
                Ok(Flow::None)
            }
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Flow, String> {
        for s in stmts {
            match self.exec(s)? {
                Flow::None => {}
                f => return Ok(f),
            }
        }
        Ok(Flow::None)
    }

    fn run_loop(&mut self, body: &[Stmt]) -> Flow {
        for s in body {
            match self.exec(s) {
                Ok(f) => match f {
                    Flow::None => {}
                    other => return other,
                },
                Err(e) => return Flow::Return(Value::String(e)),
            }
        }
        Flow::None
    }

    pub(crate) fn eval(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Float(n) => Ok(Value::Float(*n)),
            Expr::String(s) => Ok(Value::String(s.clone())),
            Expr::Boolean(b) => Ok(Value::Boolean(*b)),
            Expr::Nil => Ok(Value::Nil),
            Expr::Variable(name) => self.get(name),
            Expr::Fn { generic_params, params, body } => {
                let captured: Vec<HashMap<String, Value>> =
                    self.locals.iter().rev().cloned().collect();
                Ok(Value::Closure {
                    generic_params: generic_params.clone(),
                    params: params.clone(),
                    body: Arc::new(body.clone()),
                    captured: self.gc_captured(captured),
                    is_async: false,
                })
            }
            Expr::List(items) => {
                let mut vals = Vec::new();
                for i in items {
                    match i {
                        Expr::Spread(inner) => {
                            let val = self.eval(inner)?;
                            match val {
                                Value::List(h) => vals.extend(self.get_list_data(h).iter().cloned()),
                                _ => return Err("'...' can only be used with lists".into()),
                            }
                        }
                        _ => vals.push(self.eval(i)?),
                    }
                }
                Ok(self.gc_list(vals))
            }
            Expr::Set(items) => {
                let mut vals = Vec::new();
                for i in items {
                    match i {
                        Expr::Spread(inner) => {
                            let val = self.eval(inner)?;
                            match val {
                                Value::List(h) => vals.extend(self.get_list_data(h).iter().cloned()),
                                _ => return Err("'...' can only be used with lists".into()),
                            }
                        }
                        _ => vals.push(self.eval(i)?),
                    }
                }
                let mut seen = Vec::new();
                let mut result = Vec::new();
                for v in vals {
                    if !seen.contains(&v) {
                        seen.push(v.clone());
                        result.push(v);
                    }
                }
                Ok(self.gc_set(result))
            }
            Expr::Tuple(items) => {
                let vals: Result<Vec<Value>, String> = items.iter().map(|i| self.eval(i)).collect();
                Ok(Value::Tuple(vals?))
            }
            Expr::Dict(pairs) => {
                let mut vals = Vec::new();
                for (k, v) in pairs {
                    vals.push((self.eval(k)?, self.eval(v)?));
                }
                Ok(self.gc_dict(vals))
            }
            Expr::ListComp { expr, clauses } => {
                let mut results = Vec::new();
                self.eval_comp(expr, clauses, &mut results, &mut HashMap::new())?;
                Ok(self.gc_list(results))
            }
            Expr::SetComp { expr, clauses } => {
                let mut results = Vec::new();
                self.eval_comp(expr, clauses, &mut results, &mut HashMap::new())?;
                let mut seen = Vec::new();
                let mut unique = Vec::new();
                for v in results {
                    if !seen.contains(&v) {
                        seen.push(v.clone());
                        unique.push(v);
                    }
                }
                Ok(self.gc_set(unique))
            }
            Expr::DictComp { key, value, clauses } => {
                let mut keys = Vec::new();
                let mut vals = Vec::new();
                self.eval_dict_comp(key, value, clauses, &mut keys, &mut vals, &mut HashMap::new())?;
                let pairs: Vec<(Value, Value)> = keys.into_iter().zip(vals.into_iter()).collect();
                Ok(self.gc_dict(pairs))
            }
            Expr::Index { object, index } => {
                let obj = self.eval(object)?;
                let idx = self.eval(index)?;
                let idx_num = idx.as_float();
                let idx_f = |f: f64| -> usize { f as usize };
                match (&obj, &idx) {
                    (Value::List(h), _) if let Some(n) = idx_num => {
                        let items = self.get_list_data(*h);
                        let i = idx_f(n);
                        if i >= items.len() {
                            Err(format!("Index {} >= {}", i, items.len()))
                        } else {
                            Ok(items[i].clone())
                        }
                    }
                    (Value::Dict(h), Value::String(s)) => {
                        let pairs = self.get_dict_data(*h);
                        for (k, v) in pairs.iter() {
                            if let Value::String(ks) = k && ks.as_str() == s {
                                return Ok(v.clone());
                            }
                        }
                        Err(format!("Key '{}' not found", s))
                    }
                    (Value::Dict(h), _) => {
                        let pairs = self.get_dict_data(*h);
                        for (k, v) in pairs.iter() {
                            if *k == idx {
                                return Ok(v.clone());
                            }
                        }
                        Err(format!("Key '{}' not found", idx))
                    }
                    (Value::String(s), _) if let Some(n) = idx_num => {
                        let i = idx_f(n);
                        s.chars()
                            .nth(i)
                            .map(|c| Value::String(c.to_string()))
                            .ok_or_else(|| format!("String index {} out of bounds", i))
                    }
                    (Value::Module(bindings), Value::String(s)) => bindings
                        .get(s)
                        .cloned()
                        .ok_or_else(|| format!("Module has no '{}'", s)),
                    (Value::Tuple(items), _) if let Some(n) = idx_num => {
                        let i = idx_f(n);
                        if i >= items.len() {
                            Err(format!("Tuple index {} >= {}", i, items.len()))
                        } else {
                            Ok(items[i].clone())
                        }
                    }
                    _ => Err(format!("Cannot index '{}'", obj.type_name())),
                }
            }
            Expr::Slice { object, start, end } => {
                let obj = self.eval(object)?;
                let start = match start {
                    Some(e) => self.eval(e)?.as_float().map(|f| f as usize),
                    None => None,
                };
                let end = match end {
                    Some(e) => self.eval(e)?.as_float().map(|f| f as usize),
                    None => None,
                };
                match &obj {
                    Value::List(h) => {
                        let items = self.get_list_data(*h);
                        let s = start.unwrap_or(0);
                        let e = end.unwrap_or(items.len());
                        let e = e.min(items.len());
                        if s > e {
                            return Ok(self.gc_list(Vec::new()));
                        }
                        let sliced: Vec<Value> = items[s..e].to_vec();
                        Ok(self.gc_list(sliced))
                    }
                    Value::String(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        let start = start.unwrap_or(0);
                        let end = end.unwrap_or(chars.len());
                        let end = end.min(chars.len());
                        if start > end {
                            return Ok(Value::String(String::new()));
                        }
                        let sliced: String = chars[start..end].iter().collect();
                        Ok(Value::String(sliced))
                    }
                    _ => Err(format!("Cannot slice '{}'", obj.type_name())),
                }
            }
            Expr::Range { start, end } => {
                let (s, e) = (self.eval(start)?, self.eval(end)?);
                match (s.as_float(), e.as_float()) {
                    (Some(a), Some(b)) => Ok(Value::Range(a, b)),
                    _ => Err("Range bounds must be numbers".into()),
                }
            }
            Expr::Assignment { name, value } => {
                let val = self.eval(value)?;
                self.set(name, val.clone())?;
                Ok(val)
            }
            Expr::CompoundAssign { name, op, value } => {
                let left = self.get(name)?;
                let right = self.eval(value)?;
                let result = self.eval_binary(left, op, right)?;
                self.set(name, result.clone())?;
                Ok(result)
            }
            Expr::Grouping(inner) => self.eval(inner),
            Expr::UnaryOp { op, right } => {
                let r = self.eval(right)?;
                match op {
                    UnaryOpKind::Negate => match r {
                        Value::Int(n) => Ok(Value::Int(-n)),
                        Value::Float(n) => Ok(Value::Float(-n)),
                        _ => Err("Cannot negate non-number".into()),
                    },
                    UnaryOpKind::Not => Ok(Value::Boolean(!r.is_truthy())),
                    UnaryOpKind::BitNot => match r {
                        Value::Int(n) => Ok(Value::Int(!n)),
                        _ => Err("Bitwise NOT requires integer".into()),
                    },
                }
            }
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOpKind::And => {
                    let lv = self.eval(left)?;
                    if !lv.is_truthy() {
                        return Ok(Value::Boolean(false));
                    }
                    Ok(Value::Boolean(self.eval(right)?.is_truthy()))
                }
                BinaryOpKind::Or => {
                    let lv = self.eval(left)?;
                    if lv.is_truthy() {
                        return Ok(Value::Boolean(true));
                    }
                    Ok(Value::Boolean(self.eval(right)?.is_truthy()))
                }
                _ => {
                    let l = self.eval(left)?;
                    let r = self.eval(right)?;
                    self.eval_binary(l, op, r)
                }
            },
            Expr::Call { callee, args } => {
                let func = self.get(callee)?;
                self.call_func(callee, func, args)
            }
            Expr::StringInterp(parts) => {
                let mut out = String::new();
                for p in parts {
                    out.push_str(&self.eval(p)?.to_string());
                }
                Ok(Value::String(out))
            }
            Expr::FieldAccess { object, field } => {
                let obj_val = self.eval(object)?;
                match &obj_val {
                    Value::StructInstance { name: _, fields: h } => {
                        let fields = self.get_struct_fields(*h);
                        fields.get(field)
                            .cloned()
                            .ok_or_else(|| format!("Field '{}' not found", field))
                    }
                    Value::ClassInstance { fields: h, .. } => {
                        let fields = self.get_class_fields(*h);
                        fields.get(field)
                            .cloned()
                            .ok_or_else(|| format!("Field '{}' not found", field))
                    }
                    Value::Module(bindings) => {
                        bindings.get(field)
                            .cloned()
                            .ok_or_else(|| format!("Module has no '{}'", field))
                    }
                    Value::EnumDef { name: ename, variants } => {
                        for (vname, vfields) in variants {
                            if vname == field {
                                if vfields.is_empty() {
                                    return Ok(self.gc_struct(format!("{}.{}", ename, vname), HashMap::new()));
                                } else {
                                    // Variant with field(s) — accessed via MethodCall, not FieldAccess
                                    return Err(format!("Variant '{}' in enum '{}' has fields; use Option.Some(value) syntax", field, ename));
                                }
                            }
                        }
                        Err(format!("Variant '{}' not found in enum '{}'", field, ename))
                    }
                    _ => Err(format!("Cannot access field '{}' on '{}'", field, obj_val.type_name())),
                }
            }
            Expr::Await { value } => {
                let val = self.eval(value)?;
                match val {
                    Value::Future(shared) => {
                        let mut guard = shared.mutex.lock().unwrap();
                        while guard.is_none() {
                            guard = shared.cvar.wait(guard).unwrap();
                        }
                        guard.take().unwrap()
                    }
                    other => Err(format!("Cannot await '{}'", other.type_name())),
                }
            }
            Expr::FieldAssign { object, field, value } => {
                let obj_name = match object.as_ref() {
                    Expr::Variable(n) => Some(n.clone()),
                    _ => None,
                };
                let mut obj_val = self.eval(object)?;
                let val = self.eval(value)?;
                match &mut obj_val {
                    Value::StructInstance { fields: h, .. } => {
                        let f = self.get_struct_fields_mut(*h);
                        f.insert(field.clone(), val.clone());
                    }
                    Value::ClassInstance { fields: h, .. } => {
                        let f = self.get_class_fields_mut(*h);
                        f.insert(field.clone(), val.clone());
                    }
                    _ => return Err(format!("Cannot assign to field '{}' on '{}'", field, obj_val.type_name())),
                }
                if let Some(name) = obj_name {
                    self.set(&name, obj_val)?;
                }
                Ok(val)
            }
            Expr::Ternary { condition, then_expr, else_expr } => {
                if self.eval(condition)?.is_truthy() {
                    self.eval(then_expr)
                } else {
                    self.eval(else_expr)
                }
            }
            Expr::Try { expr } => {
                let val = self.eval(expr)?;
                match val {
                    Value::Ok(inner) => Ok(*inner),
                    Value::Err(e) => Err(self.runtime_err(format!("{}", e))),
                    Value::Some(inner) => Ok(*inner),
                    other => Err(self.runtime_err(format!("Cannot use '?' on '{}'", other.type_name()))),
                }
            }
            Expr::Super => {
                match &self.current_class {
                    Some(class_name) => {
                        let class_val = self.get(class_name)?;
                        let parent_name = match &class_val {
                            Value::ClassDef { parent: Some(p), .. } => p.clone(),
                            _ => return Err(format!("Class '{}' has no parent", class_name)),
                        };
                        Ok(Value::SuperRef(parent_name))
                    }
                    None => Err("'super' can only be used inside a class method".into()),
                }
            }
            Expr::Spread(_) => {
                Err("'...' can only be used inside list literals or function calls".into())
            }
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let obj_name = match object.as_ref() {
                    Expr::Variable(n) => Some(n.clone()),
                    _ => None,
                };
                let mut obj_val = self.eval(object)?;
                let mut all_args = vec![obj_val.clone()];
                let flat = self.eval_spread_args(args)?;
                all_args.extend(flat);
                // Handle super.method() — use parent class method with current self
                if let Value::SuperRef(parent_name) = &obj_val {
                    let parent_val = self.get(parent_name)?;
                    if let Value::ClassDef { methods, .. } = &parent_val {
                        if let Some(cm) = methods.iter().find(|m| m.name == *method) {
                            let self_val = self.locals.last()
                                .and_then(|l| l.get("self").cloned())
                                .ok_or_else(|| "'super' used without 'self'".to_string())?;
                            let mut all_args = vec![self_val.clone()];
                            let flat = self.eval_spread_args(args)?;
                            all_args.extend(flat);
                            self.locals.push(HashMap::new());
                            for ((pn, _, _), av) in cm.params.iter().zip(all_args) {
                                self.define(pn, av);
                            }
                            let old_class = self.current_class.clone();
                            self.current_class = Some(parent_name.clone());
                            let mut result = Value::Nil;
                            let last_is_expr = matches!(cm.body.last(), Some(Stmt::Expr { .. }));
                            for (i, s) in cm.body.iter().enumerate() {
                                if i + 1 == cm.body.len() && last_is_expr {
                                    if let Stmt::Expr { span: _, expr } = s {
                                        result = self.eval(expr)?;
                                    }
                                    break;
                                }
                                match self.exec(s)? {
                                    Flow::None => {}
                                    Flow::Return(v) => { result = v; break; }
                                    Flow::Break => return Err(self.trace_err("break in super method")),
                                    Flow::Continue => return Err(self.trace_err("continue in super method")),
                                }
                            }
                            self.current_class = old_class;
                            self.locals.pop();
                            return Ok(result);
                        } else {
                            return Err(format!("Parent class '{}' has no method '{}'", parent_name, method));
                        }
                    }
                }
                // Handle ClassInstance method dispatch with field mutation sync
                if let Value::ClassInstance { class_name, .. } = &obj_val {
                    let cname = class_name.clone();
                    let class_val = self.get(&cname)?;
                    if let Value::ClassDef { methods, .. } = &class_val {
                        if let Some(cm) = methods.iter().find(|m| m.name == *method) {
                            self.locals.push(HashMap::new());
                            for ((pn, _, _), av) in cm.params.iter().zip(all_args) {
                                self.define(pn, av);
                            }
                            let old_class = self.current_class.clone();
                            self.current_class = Some(cname.clone());
                            let mut result = Value::Nil;
                            let last_is_expr = matches!(cm.body.last(), Some(Stmt::Expr { .. }));
                            for (i, s) in cm.body.iter().enumerate() {
                                if i + 1 == cm.body.len() && last_is_expr {
                                    if let Stmt::Expr { span: _, expr } = s {
                                        result = self.eval(expr)?;
                                    }
                                    break;
                                }
                                match self.exec(s)? {
                                    Flow::None => {}
                                    Flow::Return(v) => { result = v; break; }
                                    Flow::Break => return Err(self.trace_err("break in method")),
                                    Flow::Continue => return Err(self.trace_err("continue in method")),
                                }
                            }
                            self.current_class = old_class;
                            // Sync mutated fields from self back to obj_val
                            if let Some(self_val) = self.locals.last().and_then(|l| l.get("self")) {
                                if let Value::ClassInstance { fields: init_h, .. } = self_val {
                                    if let Value::ClassInstance { fields: obj_h, .. } = &mut obj_val {
                                        let init_fields = self.get_class_fields(*init_h).clone();
                                        let obj_fields = self.get_class_fields_mut(*obj_h);
                                        *obj_fields = init_fields;
                                    }
                                }
                            }
                            self.locals.pop();
                            // Write mutated object back if it's a simple variable
                            if let Some(name) = &obj_name {
                                self.set(name, obj_val)?;
                            }
                            return Ok(result);
                        } else {
                            // Walk parent chain
                            match self.find_method_in_parents(&cname, method) {
                                Some(cm) => {
                                    self.locals.push(HashMap::new());
                                    for ((pn, _, _), av) in cm.params.iter().zip(all_args) {
                                        self.define(pn, av);
                                    }
                                    let old_class = self.current_class.clone();
                                    self.current_class = Some(cname.clone());
                                    let mut result = Value::Nil;
                                    let last_is_expr = matches!(cm.body.last(), Some(Stmt::Expr { .. }));
                                    for (i, s) in cm.body.iter().enumerate() {
                                        if i + 1 == cm.body.len() && last_is_expr {
                                            if let Stmt::Expr { span: _, expr } = s {
                                                result = self.eval(expr)?;
                                            }
                                            break;
                                        }
                                        match self.exec(s)? {
                                            Flow::None => {}
                                            Flow::Return(v) => { result = v; break; }
                                            Flow::Break => return Err(self.trace_err("break in method")),
                                            Flow::Continue => return Err(self.trace_err("continue in method")),
                                        }
                                    }
                                    self.current_class = old_class;
                                    if let Some(self_val) = self.locals.last().and_then(|l| l.get("self")) {
                                        if let Value::ClassInstance { fields: init_h, .. } = self_val {
                                            if let Value::ClassInstance { fields: obj_h, .. } = &mut obj_val {
                                                let init_fields = self.get_class_fields(*init_h).clone();
                                                let obj_fields = self.get_class_fields_mut(*obj_h);
                                                *obj_fields = init_fields;
                                            }
                                        }
                                    }
                                    self.locals.pop();
                                    if let Some(name) = &obj_name {
                                        self.set(name, obj_val)?;
                                    }
                                    return Ok(result);
                                }
                                None => return Err(format!("Class '{}' has no method '{}'", cname, method)),
                            }
                        }
                    } else {
                        return Err(format!("Cannot call method '{}' on '{}'", method, obj_val.type_name()));
                    }
                }
                match &mut obj_val {
                    Value::Module(bindings) => {
                        if let Some(val) = bindings.get(method) {
                            match val {
                                Value::BuiltinFn(_) | Value::Function { .. } | Value::Closure { .. } => {
                                    return self.call_func_with_values(method, val.clone(), all_args[1..].to_vec());
                                }
                                _ => {
                                    return Ok(val.clone());
                                }
                            }
                        }
                        Err(format!("Module has no '{}'", method))
                    }
                    Value::Iterator(kind) => {
                        match method.as_str() {
                            "next" => Ok(self.iter_next(kind)),
                            "map" => {
                                let func = all_args.get(1)
                                    .ok_or_else(|| "map() needs a function argument".to_string())?
                                    .clone();
                                Ok(Value::Iterator(IterKind::Map { inner: Box::new(kind.clone()), func: Box::new(func) }))
                            }
                            "filter" => {
                                let func = all_args.get(1)
                                    .ok_or_else(|| "filter() needs a function argument".to_string())?
                                    .clone();
                                Ok(Value::Iterator(IterKind::Filter { inner: Box::new(kind.clone()), func: Box::new(func) }))
                            }
                            "take" => {
                                let n_arg = all_args.get(1)
                                    .ok_or_else(|| "take() needs a number argument".to_string())?;
                                let n = n_arg.as_float()
                                    .ok_or_else(|| format!("take() expects number, got {}", n_arg.type_name()))? as usize;
                                Ok(Value::Iterator(IterKind::Take { inner: Box::new(kind.clone()), remaining: n }))
                            }
                            "skip" => {
                                let n_arg = all_args.get(1)
                                    .ok_or_else(|| "skip() needs a number argument".to_string())?;
                                let n = n_arg.as_float()
                                    .ok_or_else(|| format!("skip() expects number, got {}", n_arg.type_name()))? as usize;
                                Ok(Value::Iterator(IterKind::Skip { inner: Box::new(kind.clone()), remaining: n }))
                            }
                            "enumerate" => {
                                Ok(Value::Iterator(IterKind::Enumerate { inner: Box::new(kind.clone()), index: 0 }))
                            }
                            "zip" => {
                                let other = all_args.get(1)
                                    .ok_or_else(|| "zip() needs an iterable argument".to_string())?
                                    .clone();
                                let other_iter = self.make_iter(&other)?;
                                if let Value::Iterator(other_kind) = other_iter {
                                    Ok(Value::Iterator(IterKind::Zip {
                                        inner1: Box::new(kind.clone()),
                                        inner2: Box::new(other_kind),
                                    }))
                                } else {
                                    Err("zip(): failed to create iterator".to_string())
                                }
                            }
                            "chain" => {
                                let other = all_args.get(1)
                                    .ok_or_else(|| "chain() needs an iterable argument".to_string())?
                                    .clone();
                                let other_iter = self.make_iter(&other)?;
                                if let Value::Iterator(other_kind) = other_iter {
                                    Ok(Value::Iterator(IterKind::Chain {
                                        inner: Box::new(kind.clone()),
                                        next: Some(Box::new(other_kind)),
                                    }))
                                } else {
                                    Err("chain(): failed to create iterator".to_string())
                                }
                            }
                            "flatten" => {
                                Ok(Value::Iterator(IterKind::Flatten {
                                    inner: Box::new(kind.clone()),
                                    current_sub: None,
                                }))
                            }
                            "collect" => {
                                let mut items = Vec::new();
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => items.push(val),
                                    }
                                }
                                Ok(self.gc_list(items))
                            }
                            "fold" => {
                                let acc = all_args.get(1)
                                    .ok_or_else(|| "fold() needs an initial value".to_string())?
                                    .clone();
                                let func = all_args.get(2)
                                    .ok_or_else(|| "fold() needs a function".to_string())?
                                    .clone();
                                let mut result = acc;
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            result = self.call_func_with_values("fold callback", func.clone(), vec![result, val])?;
                                        }
                                    }
                                }
                                Ok(result)
                            }
                            "for_each" => {
                                let func = all_args.get(1)
                                    .ok_or_else(|| "for_each() needs a function".to_string())?
                                    .clone();
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            self.call_func_with_values("for_each callback", func.clone(), vec![val])?;
                                        }
                                    }
                                }
                                Ok(Value::Nil)
                            }
                            "all" => {
                                let func = all_args.get(1)
                                    .ok_or_else(|| "all() needs a predicate".to_string())?
                                    .clone();
                                let mut result = Value::Boolean(true);
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            let ok = self.call_func_with_values("all callback", func.clone(), vec![val])?;
                                            if !ok.is_truthy() {
                                                result = Value::Boolean(false);
                                                break;
                                            }
                                        }
                                    }
                                }
                                Ok(result)
                            }
                            "any" => {
                                let func = all_args.get(1)
                                    .ok_or_else(|| "any() needs a predicate".to_string())?
                                    .clone();
                                let mut result = Value::Boolean(false);
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            let ok = self.call_func_with_values("any callback", func.clone(), vec![val])?;
                                            if ok.is_truthy() {
                                                result = Value::Boolean(true);
                                                break;
                                            }
                                        }
                                    }
                                }
                                Ok(result)
                            }
                            "count" => {
                                let mut c = 0usize;
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        _ => c += 1,
                                    }
                                }
                                Ok(Value::Int(c as i64))
                            }
                            "nth" => {
                                let n_arg = all_args.get(1)
                                    .ok_or_else(|| "nth() needs a number".to_string())?;
                                let n = n_arg.as_float()
                                    .ok_or_else(|| format!("nth() expects number, got {}", n_arg.type_name()))? as usize;
                                let mut result = Value::Nil;
                                for _ in 0..=n {
                                    result = self.iter_next(kind);
                                    if matches!(result, Value::Nil) {
                                        break;
                                    }
                                }
                                Ok(result)
                            }
                            "last" => {
                                let mut result = Value::Nil;
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => result = val,
                                    }
                                }
                                Ok(result)
                            }
                            "sum" => {
                                let mut total = Value::Int(0);
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            total = self.eval_binary(total, &BinaryOpKind::Add, val)?;
                                        }
                                    }
                                }
                                Ok(total)
                            }
                            "min" => {
                                let mut result = Value::Nil;
                                let mut first = true;
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            if first {
                                                result = val;
                                                first = false;
                                            } else {
                                                let cmp = self.eval_binary(val.clone(), &BinaryOpKind::Less, result.clone())?;
                                                if cmp.is_truthy() {
                                                    result = val;
                                                }
                                            }
                                        }
                                    }
                                }
                                if first { Err("min() on empty iterator".to_string()) } else { Ok(result) }
                            }
                            "max" => {
                                let mut result = Value::Nil;
                                let mut first = true;
                                loop {
                                    let next = self.iter_next(kind);
                                    match next {
                                        Value::Nil => break,
                                        val => {
                                            if first {
                                                result = val;
                                                first = false;
                                            } else {
                                                let cmp = self.eval_binary(val.clone(), &BinaryOpKind::Greater, result.clone())?;
                                                if cmp.is_truthy() {
                                                    result = val;
                                                }
                                            }
                                        }
                                    }
                                }
                                if first { Err("max() on empty iterator".to_string()) } else { Ok(result) }
                            }
                            _ => Err(format!("Method '{}' is not supported on iterator", method)),
                        }
                    }
                    Value::EnumDef { name: ename, variants } => {
                        // Enum variant via method call: e.g., Option.Some(42)
                        // Look for the variant and return a constructor or instance
                        for (vname, vfields) in variants {
                            if vname == method {
                                if vfields.is_empty() {
                                    return Ok(self.gc_struct(format!("{}.{}", ename, vname), HashMap::new()));
                                } else {
                                    let mut field_map = HashMap::new();
                                    let evaled = self.eval_spread_args(args)?;
                                    if evaled.len() != vfields.len() {
                                        return Err(format!("Variant '{}.{}' expects {} fields, got {}",
                                            ename, vname, vfields.len(), evaled.len()));
                                    }
                                    for ((fname, _), val) in vfields.iter().zip(evaled) {
                                        field_map.insert(fname.clone(), val);
                                    }
                                    return Ok(self.gc_struct(format!("{}.{}", ename, vname), field_map));
                                }
                            }
                        }
                        Err(format!("Variant '{}' not found in enum '{}'", method, ename))
                    }
                    Value::TraitObject { trait_name, value, methods } => {
                        let inner_val = (**value).clone();
                        if let Some((_, func)) = methods.iter().find(|(n, _)| n == method) {
                            let func_name = match func {
                                Value::Function { name, .. } => name.clone(),
                                _ => method.to_string(),
                            };
                            let mut all_args = vec![inner_val];
                            let flat = self.eval_spread_args(args)?;
                            all_args.extend(flat);
                            return self.call_func_with_values(&func_name, func.clone(), all_args);
                        }
                        // Look up in the trait impl registry using the concrete type name
                        let concrete_type = match value.as_ref() {
                            Value::StructInstance { name, .. } => name.clone(),
                            Value::ClassInstance { class_name, .. } => class_name.clone(),
                            v => v.type_name().to_string(),
                        };
                        let method_func = self.trait_impls
                            .get(trait_name)
                            .and_then(|impls| impls.get(&concrete_type))
                            .and_then(|methods| methods.iter().find(|(n, _)| n == method))
                            .map(|(_, f)| f.clone());
                        if let Some(func) = method_func {
                            let func_name = match &func {
                                Value::Function { name, .. } => name.clone(),
                                _ => method.to_string(),
                            };
                            let mut all_args = vec![inner_val];
                            let flat = self.eval_spread_args(args)?;
                            all_args.extend(flat);
                            return self.call_func_with_values(&func_name, func, all_args);
                        }
                        Err(format!("Method '{}' not found for trait '{}' on type '{}'", method, trait_name, concrete_type))
                    }
                    _ => {
                        if method == "iter" {
                            let iter = self.make_iter(&obj_val)?;
                            return Ok(iter);
                        }
                        let func = self.get(method)?;
                        self.call_func_with_values(method, func, all_args)
                    }
                }
            }
        }
    }

    fn eval_comp(
        &mut self,
        expr: &Expr,
        clauses: &[CompClause],
        results: &mut Vec<Value>,
        bindings: &mut HashMap<String, Value>,
    ) -> Result<(), String> {
        if clauses.is_empty() {
            self.locals.push(bindings.clone());
            let val = self.eval(expr)?;
            self.locals.pop();
            results.push(val);
            return Ok(());
        }
        let clause = &clauses[0];
        let rest = &clauses[1..];
        let iterable = self.eval(&clause.iterable)?;
        let items: Vec<Value> = match &iterable {
            Value::List(h) => self.get_list_data(*h).clone(),
            Value::Set(h) => self.get_list_data(*h).clone(),
            Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
            Value::Range(s, e) => ((*s as i64)..(*e as i64)).map(Value::Int).collect(),
            _ => return Err(format!("Cannot iterate over '{}'", iterable.type_name())),
        };
        for item in items {
            bindings.insert(clause.var.clone(), item);
            let mut ok = true;
            for cond in &clause.conditions {
                self.locals.push(bindings.clone());
                let val = self.eval(cond)?;
                self.locals.pop();
                if !val.is_truthy() {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            if rest.is_empty() {
                self.locals.push(bindings.clone());
                let val = self.eval(expr)?;
                self.locals.pop();
                results.push(val);
            } else {
                self.eval_comp(expr, rest, results, bindings)?;
            }
        }
        Ok(())
    }

    fn eval_dict_comp(
        &mut self,
        key: &Expr,
        value: &Expr,
        clauses: &[CompClause],
        keys: &mut Vec<Value>,
        vals: &mut Vec<Value>,
        bindings: &mut HashMap<String, Value>,
    ) -> Result<(), String> {
        if clauses.is_empty() {
            self.locals.push(bindings.clone());
            let k = self.eval(key)?;
            let v = self.eval(value)?;
            self.locals.pop();
            keys.push(k);
            vals.push(v);
            return Ok(());
        }
        let clause = &clauses[0];
        let rest = &clauses[1..];
        let iterable = self.eval(&clause.iterable)?;
        let items: Vec<Value> = match &iterable {
            Value::List(h) => self.get_list_data(*h).clone(),
            Value::Set(h) => self.get_list_data(*h).clone(),
            Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
            Value::Range(s, e) => ((*s as i64)..(*e as i64)).map(Value::Int).collect(),
            _ => return Err(format!("Cannot iterate over '{}'", iterable.type_name())),
        };
        for item in items {
            bindings.insert(clause.var.clone(), item);
            let mut ok = true;
            for cond in &clause.conditions {
                self.locals.push(bindings.clone());
                let val = self.eval(cond)?;
                self.locals.pop();
                if !val.is_truthy() {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            if rest.is_empty() {
                self.locals.push(bindings.clone());
                let k = self.eval(key)?;
                let v = self.eval(value)?;
                self.locals.pop();
                keys.push(k);
                vals.push(v);
            } else {
                self.eval_dict_comp(key, value, rest, keys, vals, bindings)?;
            }
        }
        Ok(())
    }

    fn call_func(&mut self, name: &str, func: Value, args: &[Expr]) -> Result<Value, String> {
        // If async, spawn a thread and return a Future
        if let Value::Function { is_async: true, params, body, .. }
            | Value::Closure { is_async: true, params, body, .. } = &func
        {
            let evaled = self.eval_spread_args(args)?;
            let captured: Vec<HashMap<String, Value>> = match &func {
                Value::Closure { captured: h, .. } => self.get_captured_data(*h).clone(),
                _ => Vec::new(),
            };
            let params_clone = params.clone();
            let body_clone = body.clone();
            let shared: SharedFuture = Arc::new(FutureState::new());
            let shared_clone = shared.clone();
            std::thread::spawn(move || {
                let mut interp = crate::interpreter::Interpreter::new();
                interp.locals = captured.clone();
                interp.locals.push(HashMap::new());
                for (i, (pn, _, pd)) in params_clone.iter().enumerate() {
                    if let Some(av) = evaled.get(i) {
                        interp.define(pn, av.clone());
                    } else if let Some(def_expr) = pd {
                        let val = interp.eval(def_expr).unwrap_or(Value::Nil);
                        interp.define(pn, val);
                    }
                }
                let result = interp.exec_block(&body_clone)
                    .map(|f| match f { Flow::None => Value::Nil, Flow::Return(v) => v, _ => Value::Nil })
                    .or_else(|e| Err(e));
                let mut guard = shared_clone.mutex.lock().unwrap();
                *guard = Some(result);
                shared_clone.cvar.notify_one();
            });
            return Ok(Value::Future(shared));
        }
        match &func {
            Value::Function { params, .. } | Value::Closure { params, .. } => {
                let evaled = self.eval_spread_args(args)?;
                let has_defaults = params.iter().any(|(_, _, d)| d.is_some());
                if evaled.len() > params.len() || (!has_defaults && evaled.len() != params.len()) {
                    let msg = if has_defaults {
                        format!("'{}' expects at most {} args, got {}", name, params.len(), evaled.len())
                    } else {
                        format!("'{}' expects {} args, got {}", name, params.len(), evaled.len())
                    };
                    return Err(msg);
                }
                self.call_func_with_values(name, func, evaled)
            }
            Value::StructDef { name: sname, fields } => {
                let evaled = self.eval_spread_args(args)?;
                if evaled.len() != fields.len() {
                    return Err(format!("'{}' expects {} fields, got {}", sname, fields.len(), evaled.len()));
                }
                let mut field_map = HashMap::new();
                for ((fname, _), val) in fields.iter().zip(evaled) {
                    field_map.insert(fname.clone(), val);
                }
                Ok(self.gc_struct(sname.clone(), field_map))
            }
            Value::ClassDef { name: cname, methods, .. } => {
                let evaled = self.eval_spread_args(args)?;
                let mut instance = self.gc_class(cname.clone(), HashMap::new());
                // Call __init__ if present (walk parent chain)
                let init = methods.iter().find(|m| m.name == "__init__")
                    .map(|m| m.clone())
                    .or_else(|| self.find_method_in_parents(cname, "__init__"));
                if let Some(init) = init {
                    let mut init_args = vec![instance.clone()];
                    init_args.extend(evaled);
                    if init_args.len() != init.params.len() {
                        return Err(format!("'{}.__init__' expects {} args, got {}",
                            cname, init.params.len() - 1, init_args.len() - 1));
                    }
                    self.locals.push(HashMap::new());
                    for ((pn, _, _), av) in init.params.iter().zip(init_args) {
                        self.define(pn, av);
                    }
                    let old_class = self.current_class.clone();
                    self.current_class = Some(cname.clone());
                    for (i, s) in init.body.iter().enumerate() {
                        let last_is_expr = matches!(init.body.last(), Some(Stmt::Expr { .. }));
                        if i + 1 == init.body.len() && last_is_expr {
                            if let Stmt::Expr { span: _, expr } = s {
                                self.eval(expr)?;
                            }
                            break;
                        }
                        if let Flow::Return(_) = self.exec(s)? { break; }
                    }
                    // Sync mutated fields from self back to instance
                    if let Some(self_val) = self.locals.last().and_then(|l| l.get("self")) {
                        if let Value::ClassInstance { fields: init_h, .. } = self_val {
                            if let Value::ClassInstance { fields: instance_h, .. } = &mut instance {
                                let init_fields = self.get_class_fields(*init_h).clone();
                                let instance_fields = self.get_class_fields_mut(*instance_h);
                                *instance_fields = init_fields;
                            }
                        }
                    }
                    self.current_class = old_class;
                    self.locals.pop();
                }
                Ok(instance)
            }
            Value::BuiltinFn(bn) => {
                match bn.as_str() {
                    "map" | "filter" | "fold" | "take" | "collect" | "iter" | "as_trait" => {
                        let evaled = self.eval_spread_args(args)?;
                        self.call_special_builtin(bn, evaled)
                    }
                    _ => {
                        let evaled = self.eval_spread_args(args)?;
                        call_builtin(bn, evaled, &mut self.gc)
                    }
                }
            }
            _ => Err(format!("'{}' is not a function", name)),
        }
    }

    fn find_method_in_parents(&self, class_name: &str, method: &str) -> Option<ClassMethod> {
        let mut current = class_name.to_string();
        loop {
            let class_val = self.get(&current).ok()?;
            let parent = match &class_val {
                Value::ClassDef { parent: Some(p), .. } => p.clone(),
                _ => return None,
            };
            let parent_val = self.get(&parent).ok()?;
            if let Value::ClassDef { methods, .. } = &parent_val {
                if let Some(cm) = methods.iter().find(|m| m.name == method) {
                    return Some(cm.clone());
                }
            }
            current = parent;
        }
    }

    fn call_func_with_values(
        &mut self,
        name: &str,
        func: Value,
        evaled: Vec<Value>,
    ) -> Result<Value, String> {
        match &func {
                Value::Function { params, body, .. } | Value::Closure { params, body, .. } => {
                let has_defaults = params.iter().any(|(_, _, d)| d.is_some());
                if evaled.len() > params.len() || (!has_defaults && evaled.len() != params.len()) {
                    let msg = if has_defaults {
                        format!("'{}' expects at most {} args, got {}", name, params.len(), evaled.len())
                    } else {
                        format!("'{}' expects {} args, got {}", name, params.len(), evaled.len())
                    };
                    return Err(msg);
                }

                // Generator: spawn thread with rendezvous channel
                if contains_yield(body) {
                    let (tx, rx) = sync_channel(0);
                    let captured: Vec<HashMap<String, Value>> = match &func {
                        Value::Closure { captured: h, .. } => self.get_captured_data(*h).clone(),
                        _ => Vec::new(),
                    };
                    let params_clone = params.clone();
                    let body_clone = body.clone();

                    // Try JIT-compiled generator
                    if self.jit_enabled {
                        let jit = self.jit.get_or_insert_with(JitEngine::new);
                        if let Some(jit_fn) = jit.try_compile(name, params, &body_clone) {
                            thread::spawn(move || {
                                let mut interp = Interpreter::new();
                                interp.generator_channel = Some(tx);
                                for scope in captured.iter().rev() {
                                    interp.locals.push(scope.clone());
                                }
                                interp.locals.push(HashMap::new());
                                let mut args: Vec<*mut Value> = evaled.into_iter()
                                    .map(|v| Box::into_raw(Box::new(v)))
                                    .collect();
                                unsafe { jit_fn(&mut interp as *mut _ as *mut c_void, args.as_mut_ptr(), args.len()); }
                                for arg in args {
                                    unsafe { drop(Box::from_raw(arg)); }
                                }
                            });
                            return Ok(Value::Iterator(IterKind::Generator {
                                rx: Arc::new(Mutex::new(rx)),
                                exhausted: false,
                            }));
                        }
                    }

                    // Fallback: interpreter
                    let evaled_clone = evaled.clone();
                    thread::spawn(move || {
                        let mut interp = Interpreter::new();
                        interp.generator_channel = Some(tx);
                        for scope in captured.iter().rev() {
                            interp.locals.push(scope.clone());
                        }
                        interp.locals.push(HashMap::new());
                        for (i, (pn, _, pd)) in params_clone.iter().enumerate() {
                            if let Some(av) = evaled_clone.get(i) {
                                interp.define(pn, av.clone());
                            } else if let Some(def_expr) = pd {
                                let val = interp.eval(def_expr).unwrap_or(Value::Nil);
                                interp.define(pn, val);
                            }
                        }
                        let _ = interp.exec_block(&body_clone);
                    });
                    return Ok(Value::Iterator(IterKind::Generator {
                        rx: Arc::new(Mutex::new(rx)),
                        exhausted: false,
                    }));
                }

                if self.jit_enabled && matches!(&func, Value::Function { .. }) {
                    let jit = self.jit.get_or_insert_with(JitEngine::new);
                    if let Some(jit_fn) = jit.try_compile(name, params, body) {
                        let mut args: Vec<*mut Value> = evaled.into_iter()
                            .map(|v| Box::into_raw(Box::new(v)))
                            .collect();
                        JIT_INTERP.set(Some(self as *mut Interpreter));
                        let result_ptr = unsafe { jit_fn(self as *mut Self as *mut c_void, args.as_mut_ptr(), args.len()) };
                        JIT_INTERP.set(None);
                        let result = unsafe { (*result_ptr).clone() };
                        unsafe { drop(Box::from_raw(result_ptr)); }
                        for arg in args {
                            if arg != result_ptr {
                                unsafe { drop(Box::from_raw(arg)); }
                            }
                        }
                        return Ok(result);
                    }
                }

                if let Value::Closure { captured: h, .. } = &func {
                    let captured = self.get_captured_data(*h).clone();
                    for scope in captured.iter().rev() {
                        self.locals.push(scope.clone());
                    }
                }
                self.locals.push(HashMap::new());
                for (i, (pn, _, pd)) in params.iter().enumerate() {
                    if let Some(av) = evaled.get(i) {
                        self.define(pn, av.clone());
                    } else if let Some(def_expr) = pd {
                        let val = self.eval(def_expr)?;
                        self.define(pn, val);
                    }
                }
                let mut result = Value::Nil;
                let last_is_expr = matches!(body.last(), Some(Stmt::Expr { .. }));
                self.debug_before_call();
                self.push_call(name);
                let body_result = (|| {
                    for (i, s) in body.iter().enumerate() {
                        if i + 1 == body.len() && last_is_expr {
                            if let Stmt::Expr { span: _, expr } = s {
                                self.debug_before_stmt(s);
                                result = self.eval(expr)?;
                            }
                            break;
                        }
                        match self.exec(s)? {
                            Flow::None => {}
                            Flow::Return(v) => {
                                result = v;
                                break;
                            }
                            Flow::Break => {
                                return Err(self.trace_err("break in function"));
                            }
                            Flow::Continue => {
                                return Err(self.trace_err("continue in function"));
                            }
                        }
                    }
                    Ok(result)
                })();
                self.pop_call();
                self.debug_after_call();
                let result = body_result?;
                let pop_count = 1
                    + if let Value::Closure { captured: h, .. } = &func {
                        self.get_captured_data(*h).len()
                    } else {
                        0
                    };
                for _ in 0..pop_count {
                    self.locals.pop();
                }
                Ok(result)
            }
            Value::BuiltinFn(bn) => {
                if bn.starts_with("stdlib.") {
                    let rest = bn.strip_prefix("stdlib.").unwrap();
                    let (mod_name, fn_name) = rest.split_once('.').unwrap_or((rest, ""));
                    call_stdlib(mod_name, fn_name, &evaled, &mut self.gc)
                } else {
                    call_builtin(bn, evaled, &mut self.gc)
                }
            }
            _ => Err(format!("'{}' is not a function", name)),
        }
    }

    fn make_iter(&self, val: &Value) -> Result<Value, String> {
        match val {
            Value::List(h) => Ok(Value::Iterator(IterKind::List {
                handle: *h,
                index: 0,
            })),
            Value::Set(h) => Ok(Value::Iterator(IterKind::List {
                handle: *h,
                index: 0,
            })),
            Value::String(s) => Ok(Value::Iterator(IterKind::String {
                chars: s.chars().map(|c| c.to_string()).collect(),
                index: 0,
            })),
            Value::Range(s, e) => Ok(Value::Iterator(IterKind::Range {
                start: *s,
                end: *e,
                current: *s,
            })),
            Value::Iterator(_) => Ok(val.clone()),
            _ => Err(format!("Cannot iterate over '{}'", val.type_name())),
        }
    }

    pub(crate) fn iter_next(&mut self, kind: &mut IterKind) -> Value {
        match kind {
            IterKind::List { handle: h, index } => {
                let items = self.get_list_data(*h);
                if *index < items.len() {
                    let val = items[*index].clone();
                    *index += 1;
                    val
                } else {
                    Value::Nil
                }
            }
            IterKind::String { chars, index } => {
                if *index < chars.len() {
                    let val = Value::String(chars[*index].clone());
                    *index += 1;
                    val
                } else {
                    Value::Nil
                }
            }
            IterKind::Generator { rx, exhausted } => {
                if *exhausted {
                    return Value::Nil;
                }
                match rx.lock().unwrap().recv() {
                    Ok(val) => val,
                    Err(_) => {
                        *exhausted = true;
                        Value::Nil
                    }
                }
            }
            IterKind::Range { start: _start, end, current } => {
                if *current < *end {
                    let val = Value::Float(*current);
                    *current += 1.0;
                    val
                } else {
                    Value::Nil
                }
            }
            IterKind::Map { inner, func } => {
                let next = self.iter_next(inner);
                match next {
                    Value::Nil => Value::Nil,
                    val => {
                        match self.try_call_func((**func).clone(), vec![val]) {
                            Ok(v) => v,
                            Err(_) => Value::Nil,
                        }
                    }
                }
            }
            IterKind::Filter { inner, func } => {
                loop {
                    let next = self.iter_next(inner);
                    match next {
                        Value::Nil => return Value::Nil,
                        val => {
                            match self.try_call_func((**func).clone(), vec![val.clone()]) {
                                Ok(ok) if ok.is_truthy() => return val,
                                _ => continue,
                            }
                        }
                    }
                }
            }
            IterKind::Take { inner, remaining } => {
                if *remaining > 0 {
                    let next = self.iter_next(inner);
                    match next {
                        Value::Nil => Value::Nil,
                        val => {
                            *remaining -= 1;
                            val
                        }
                    }
                } else {
                    Value::Nil
                }
            }
            IterKind::Skip { inner, remaining } => {
                while *remaining > 0 {
                    let next = self.iter_next(inner);
                    match next {
                        Value::Nil => return Value::Nil,
                        _ => *remaining -= 1,
                    }
                }
                self.iter_next(inner)
            }
            IterKind::Enumerate { inner, index } => {
                let next = self.iter_next(inner);
                match next {
                    Value::Nil => Value::Nil,
                    val => {
                        let idx = *index;
                        *index += 1;
                        Value::Tuple(vec![Value::Int(idx as i64), val])
                    }
                }
            }
            IterKind::Zip { inner1, inner2 } => {
                let v1 = self.iter_next(inner1);
                let v2 = self.iter_next(inner2);
                match (v1, v2) {
                    (Value::Nil, _) | (_, Value::Nil) => Value::Nil,
                    (a, b) => Value::Tuple(vec![a, b]),
                }
            }
            IterKind::Chain { inner, next } => {
                let val = self.iter_next(inner);
                match val {
                    Value::Nil => {
                        if let Some(n) = next {
                            let result = self.iter_next(n);
                            if matches!(result, Value::Nil) {
                                *next = None;
                            }
                            result
                        } else {
                            Value::Nil
                        }
                    }
                    v => v,
                }
            }
            IterKind::Flatten { inner, current_sub } => {
                loop {
                    if let Some(sub) = current_sub {
                        let val = self.iter_next(sub);
                        match val {
                            Value::Nil => {
                                *current_sub = None;
                                continue;
                            }
                            v => return v,
                        }
                    } else {
                        let next = self.iter_next(inner);
                        match next {
                            Value::Nil => return Value::Nil,
                            val => {
                                let iter = self.make_iter(&val);
                                match iter {
                                    Ok(Value::Iterator(sub_kind)) => {
                                        *current_sub = Some(Box::new(sub_kind));
                                        continue;
                                    }
                                    _ => return Value::Nil,
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn try_call_func(&mut self, func: Value, args: Vec<Value>) -> Result<Value, String> {
        match &func {
            Value::BuiltinFn(bn) => call_builtin(bn, args, &mut self.gc),
            Value::Function { .. } | Value::Closure { .. } => self.call_func_with_values("iterator callback", func, args),
            _ => Err(format!("'{}' is not callable", func.type_name())),
        }
    }

    fn call_special_builtin(&mut self, name: &str, evaled: Vec<Value>) -> Result<Value, String> {
        match name {
            "iter" => self.make_iter(evaled.first().ok_or("iter() expects 1 arg")?),
            "map" => {
                if evaled.len() != 2 {
                    return Err("map(fn, list) expects 2 args".into());
                }
                let func = evaled[0].clone();
                let list = match &evaled[1] {
                    Value::List(h) => self.get_list_data(*h).clone(),
                    v => return Err(format!("map() expects list, got {}", v.type_name())),
                };
                let mut result = Vec::new();
                for item in list.iter() {
                    let val = self.call_func_with_values("map callback", func.clone(), vec![item.clone()])?;
                    result.push(val);
                }
                Ok(self.gc_list(result))
            }
            "filter" => {
                if evaled.len() != 2 {
                    return Err("filter(fn, list) expects 2 args".into());
                }
                let func = evaled[0].clone();
                let list = match &evaled[1] {
                    Value::List(h) => self.get_list_data(*h).clone(),
                    v => return Err(format!("filter() expects list, got {}", v.type_name())),
                };
                let mut result = Vec::new();
                for item in list.iter() {
                    let val = self.call_func_with_values("filter callback", func.clone(), vec![item.clone()])?;
                    if val.is_truthy() {
                        result.push(item.clone());
                    }
                }
                Ok(self.gc_list(result))
            }
            "fold" => {
                if evaled.len() != 3 {
                    return Err("fold(fn, init, list) expects 3 args".into());
                }
                let func = evaled[0].clone();
                let mut acc = evaled[1].clone();
                let list = match &evaled[2] {
                    Value::List(h) => self.get_list_data(*h).clone(),
                    v => return Err(format!("fold() expects list, got {}", v.type_name())),
                };
                for item in list.iter() {
                    acc = self.call_func_with_values("fold callback", func.clone(), vec![acc, item.clone()])?;
                }
                Ok(acc)
            }
            "take" => {
                if evaled.len() != 2 {
                    return Err("take(n, list) expects 2 args".into());
                }
                let n = match &evaled[0] {
                    Value::Int(x) => *x as usize,
                    Value::Float(x) => *x as usize,
                    v => return Err(format!("take() expects number, got {}", v.type_name())),
                };
                let list = match &evaled[1] {
                    Value::List(h) => self.get_list_data(*h).clone(),
                    v => return Err(format!("take() expects list, got {}", v.type_name())),
                };
                let result: Vec<Value> = list.iter().take(n).cloned().collect();
                Ok(self.gc_list(result))
            }
            "collect" => {
                if evaled.len() != 1 {
                    return Err("collect() expects 1 arg".into());
                }
                let mut iter_val = evaled.into_iter().next().unwrap();
                match &mut iter_val {
                    Value::Iterator(kind) => {
                        let mut items = Vec::new();
                        loop {
                            let next = self.iter_next(kind);
                            match next {
                                Value::Nil => break,
                                val => items.push(val),
                            }
                        }
                        Ok(self.gc_list(items))
                    }
                    v => Err(format!("collect() expects iterator, got {}", v.type_name())),
                }
            }
            "as_trait" => {
                if evaled.len() != 2 {
                    return Err("as_trait(value, trait_name) expects 2 args".into());
                }
                let value = evaled[0].clone();
                let trait_name = match &evaled[1] {
                    Value::String(s) => s.clone(),
                    v => return Err(format!("as_trait() expects trait name string, got {}", v.type_name())),
                };
                let concrete_type = match &value {
                    Value::StructInstance { name, .. } => name.clone(),
                    Value::ClassInstance { class_name, .. } => class_name.clone(),
                    v => v.type_name().to_string(),
                };
                let methods_raw = self.trait_impls
                    .get(&trait_name)
                    .and_then(|impls| impls.get(&concrete_type));
                let methods = methods_raw
                    .cloned()
                    .ok_or_else(|| format!("Type '{}' does not implement trait '{}'", concrete_type, trait_name))?;
                Ok(Value::TraitObject {
                    trait_name,
                    value: Box::new(value),
                    methods,
                })
            }
            _ => Err(format!("Unknown special builtin '{}'", name)),
        }
    }

    fn eval_binary(
        &mut self,
        left: Value,
        op: &BinaryOpKind,
        right: Value,
    ) -> Result<Value, String> {
        match op {
            BinaryOpKind::Add => match (&left, &right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + *b as f64)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                (Value::String(a), b) => Ok(Value::String(format!("{}{}", a, b))),
                (a, Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
                (Value::List(h_a), Value::List(h_b)) => {
                    let mut m = self.get_list_data(*h_a).clone();
                    m.extend(self.get_list_data(*h_b).iter().cloned());
                    Ok(self.gc_list(m))
                }
                _ => Err(format!("Cannot add '{}' and '{}'", left, right)),
            },
            BinaryOpKind::Subtract => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => Ok(Value::Float(a - b)),
                _ => Err(format!("Cannot subtract '{}' and '{}'", left, right)),
            },
            BinaryOpKind::Multiply => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => Ok(Value::Float(a * b)),
                _ => Err(format!("Cannot multiply '{}' and '{}'", left, right)),
            },
            BinaryOpKind::Divide => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => {
                    if b == 0.0 {
                        Err("Division by zero".into())
                    } else {
                        Ok(Value::Float(a / b))
                    }
                }
                _ => Err(format!("Cannot divide '{}' and '{}'", left, right)),
            },
            BinaryOpKind::Modulo => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => {
                    if b == 0.0 {
                        Err("Modulo by zero".into())
                    } else {
                        Ok(Value::Float(a % b))
                    }
                }
                _ => Err(format!("Cannot modulo '{}' and '{}'", left, right)),
            },
            BinaryOpKind::Equal => Ok(Value::Boolean(left == right)),
            BinaryOpKind::NotEqual => Ok(Value::Boolean(left != right)),
            BinaryOpKind::Less => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => Ok(Value::Boolean(a < b)),
                _ => match (&left, &right) {
                    (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a < b)),
                    _ => Err(format!("Cannot compare '{}' and '{}'", left, right)),
                },
            },
            BinaryOpKind::LessEqual => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => Ok(Value::Boolean(a <= b)),
                _ => match (&left, &right) {
                    (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a <= b)),
                    _ => Err(format!("Cannot compare '{}' and '{}'", left, right)),
                },
            },
            BinaryOpKind::Greater => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => Ok(Value::Boolean(a > b)),
                _ => match (&left, &right) {
                    (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a > b)),
                    _ => Err(format!("Cannot compare '{}' and '{}'", left, right)),
                },
            },
            BinaryOpKind::GreaterEqual => match (left.as_float(), right.as_float()) {
                (Some(a), Some(b)) => Ok(Value::Boolean(a >= b)),
                _ => match (&left, &right) {
                    (Value::String(a), Value::String(b)) => Ok(Value::Boolean(a >= b)),
                    _ => Err(format!("Cannot compare '{}' and '{}'", left, right)),
                },
            },
            BinaryOpKind::And | BinaryOpKind::Or => unreachable!(),
            BinaryOpKind::BitAnd => Self::bitwise_op(left, right, |a, b| a & b),
            BinaryOpKind::BitOr => Self::bitwise_op(left, right, |a, b| a | b),
            BinaryOpKind::BitXor => Self::bitwise_op(left, right, |a, b| a ^ b),
            BinaryOpKind::ShiftLeft => Self::bitwise_op(left, right, |a, b| a << b),
            BinaryOpKind::ShiftRight => Self::bitwise_op(left, right, |a, b| a >> b),
            BinaryOpKind::In => match &right {
                Value::List(h) => Ok(Value::Boolean(self.get_list_data(*h).contains(&left))),
                Value::Set(h) => Ok(Value::Boolean(self.get_set_data(*h).contains(&left))),
                Value::String(s) => {
                    if let Value::String(ch) = &left {
                        Ok(Value::Boolean(s.contains(ch)))
                    } else {
                        Ok(Value::Boolean(false))
                    }
                }
                Value::Dict(h) => {
                    let pairs = self.get_dict_data(*h);
                    let found = pairs.iter().any(|(k, _)| k == &left);
                    Ok(Value::Boolean(found))
                }
                _ => Err(format!("'in' not supported on '{}'", right.type_name())),
            },
        }
    }

    fn bitwise_op(left: Value, right: Value, f: fn(i64, i64) -> i64) -> Result<Value, String> {
        match (left.as_float(), right.as_float()) {
            (Some(a), Some(b)) => {
                let a = a as i64;
                let b = b as i64;
                Ok(Value::Int(f(a, b)))
            }
            _ => Err("Bitwise operations require numbers".into()),
        }
    }

    fn eval_spread_args(&mut self, args: &[Expr]) -> Result<Vec<Value>, String> {
        let mut values = Vec::new();
        for arg in args {
            match arg {
                Expr::Spread(inner) => {
                    let val = self.eval(inner)?;
                    match val {
                        Value::List(h) => values.extend(self.get_list_data(h).iter().cloned()),
                        _ => return Err("'...' can only be used with lists".into()),
                    }
                }
                _ => values.push(self.eval(arg)?),
            }
        }
        Ok(values)
    }

    fn exec_import_flat_source(&mut self, source: &str) -> Result<(), String> {
        let mut stmts = Parser::new(Lexer::new(source).tokenize()).parse()?;
        stmts = self.expand_macros(&stmts)?;
        for s in &stmts {
            match self.exec(s)? {
                Flow::None => {}
                f => return Err(format!("Unexpected flow in module: {:?}", f)),
            }
        }
        Ok(())
    }

    fn exec_import_module_source(&mut self, source: &str) -> Result<Value, String> {
        let mut stmts = Parser::new(Lexer::new(source).tokenize()).parse()?;
        stmts = self.expand_macros(&stmts)?;

        // Collect public names from AST
        let has_pub = stmts.iter().any(|s| matches!(s,
            Stmt::Let { pub_flag: true, .. }
            | Stmt::Fn { pub_flag: true, .. }
            | Stmt::Struct { pub_flag: true, .. }
            | Stmt::Enum { pub_flag: true, .. }
            | Stmt::Class { pub_flag: true, .. }
            | Stmt::Trait { pub_flag: true, .. }
            | Stmt::Import { pub_flag: true, .. }
        ));
        let public_names: std::collections::HashSet<String> = if has_pub {
            stmts.iter().filter_map(|s| match s {
                Stmt::Let { pub_flag: true, name, .. } => Some(name.clone()),
                Stmt::Fn { pub_flag: true, name, .. } => Some(name.clone()),
                Stmt::Struct { pub_flag: true, name, .. } => Some(name.clone()),
                Stmt::Enum { pub_flag: true, name, .. } => Some(name.clone()),
                Stmt::Class { pub_flag: true, name, .. } => Some(name.clone()),
                Stmt::Trait { pub_flag: true, name, .. } => Some(name.clone()),
                Stmt::Import { span: _, pub_flag: true, alias, path } => {
                    Some(alias.clone().unwrap_or_else(|| {
                        path.rsplit('/').next().unwrap_or(path).to_string()
                    }))
                }
                _ => None,
            }).collect()
        } else {
            std::collections::HashSet::new()
        };

        self.locals.push(HashMap::new());
        for s in &stmts {
            match self.exec(s)? {
                Flow::None => {}
                f => return Err(format!("Unexpected flow in module: {:?}", f)),
            }
        }
        let module_scope = self.locals.pop().unwrap();
        if has_pub {
            let filtered: HashMap<String, Value> = module_scope.into_iter()
                .filter(|(k, _)| public_names.contains(k))
                .collect();
            Ok(Value::Module(filtered))
        } else {
            Ok(Value::Module(module_scope))
        }
    }

    pub(crate) fn match_pattern(&mut self, pattern: &MatchPattern, value: &Value, bindings: &mut HashMap<String, Value>) -> Result<bool, String> {
        match pattern {
            MatchPattern::Wildcard => Ok(true),
            MatchPattern::Binding(name) => {
                bindings.insert(name.clone(), value.clone());
                Ok(true)
            }
            MatchPattern::Literal(pat_expr) => {
                let pat_val = self.eval(pat_expr)?;
                Ok(*value == pat_val)
            }
            MatchPattern::Destructure(name, fields) => {
                match value {
                    Value::StructInstance { name: sname, fields: h } => {
                        // Match both short name (enum variant: "Some") and full name (struct: "Option.Some")
                        let full_name = name.clone();
                        let matches = sname == &full_name || sname.ends_with(&format!(".{}", full_name));
                        if !matches { return Ok(false); }
                        let sfields = self.get_struct_fields(*h);
                        for fname in fields {
                            if fname == "_" { continue; }
                            let field_val = sfields.get(fname)
                                .ok_or_else(|| format!("Pattern field '{}' not found in struct '{}'", fname, name))?;
                            bindings.insert(fname.clone(), field_val.clone());
                        }
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            MatchPattern::Or(patterns) => {
                for p in patterns {
                    if self.match_pattern(p, value, bindings)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    pub(crate) fn jit_exec_match(&mut self, val: &Value, arms: &[MatchArm]) -> Result<Flow, String> {
        for arm in arms {
            let mut bindings = HashMap::new();
            let matched = self.match_pattern(&arm.pattern, val, &mut bindings)?;
            if matched {
                if let Some(guard) = &arm.guard {
                    self.locals.push(HashMap::new());
                    for (k, v) in &bindings {
                        self.define(k, v.clone());
                    }
                    let guard_val = self.eval(guard)?;
                    self.locals.pop();
                    if !guard_val.is_truthy() { continue; }
                }
                self.locals.push(HashMap::new());
                for (k, v) in &bindings {
                    self.define(k, v.clone());
                }
                for s in &arm.body {
                    match self.exec(s)? {
                        Flow::None => {}
                        f => {
                            self.locals.pop();
                            return Ok(f);
                        }
                    }
                }
                self.locals.pop();
                return Ok(Flow::None);
            }
        }
        Ok(Flow::None)
    }

    pub(crate) fn jit_get_global(&self, name: &str) -> Result<Value, String> {
        self.get(name)
    }

    pub(crate) fn jit_set_global(&mut self, name: &str, value: Value) -> Result<(), String> {
        self.set(name, value)
    }

    pub(crate) fn jit_exec_nested_stmt(&mut self, stmt: &Stmt) -> Result<Flow, String> {
        self.exec(stmt)
    }

    pub(crate) fn jit_exec_try(&mut self, body: &[Stmt], catch_var: &str, catch_body: &[Stmt]) -> Result<Flow, String> {
        let ok = self.exec_block(body);
        match ok {
            Ok(Flow::None) => Ok(Flow::None),
            Ok(Flow::Return(v)) => Ok(Flow::Return(v)),
            Ok(Flow::Break) => Ok(Flow::Break),
            Ok(Flow::Continue) => Ok(Flow::Continue),
            Err(e) => {
                self.locals.push(HashMap::new());
                self.define(catch_var, Value::String(e));
                let result = self.exec_block(catch_body);
                self.locals.pop();
                result.map(|_| Flow::None)
            }
        }
    }

    pub(crate) fn jit_make_closure(&mut self, params: &[(String, Option<Type>, Option<Expr>)], body: &[Stmt], generic_params: &[String]) -> Value {
        let captured: Vec<HashMap<String, Value>> =
            self.locals.iter().rev().cloned().collect();
        Value::Closure {
            generic_params: generic_params.to_vec(),
            params: params.to_vec(),
            body: Arc::new(body.to_vec()),
            captured: self.gc_captured(captured),
            is_async: false,
        }
    }

    fn check_type(&self, value: &Value, expected: &Type) -> bool {
        match (value, expected) {
            (Value::Tuple(items), Type::Tuple(ts)) => {
                if items.len() != ts.len() { return false; }
                items.iter().zip(ts.iter()).all(|(v, t)| self.check_type(v, t))
            }
            (Value::Range(_, _), Type::Range) => true,
            (Value::Float(_), Type::Int) | (Value::Float(_), Type::Float) => true,
            (Value::String(_), Type::String) => true,
            (Value::Boolean(_), Type::Bool) => true,
            (Value::Nil, Type::Nil) => true,
            (Value::List(h), Type::List(inner)) => {
                let items = self.get_list_data(*h);
                if **inner == Type::Any {
                    return true;
                }
                items.iter().all(|v| self.check_type(v, inner))
            }
            (Value::Dict(h), Type::Dict(kt, vt)) => {
                let pairs = self.get_dict_data(*h);
                if **kt == Type::Any && **vt == Type::Any {
                    return true;
                }
                pairs
                    .iter()
                    .all(|(vk, vv)| self.check_type(vk, kt) && self.check_type(vv, vt))
            }
            (Value::Function { .. } | Value::Closure { .. }, Type::Fn(_, _)) => true,
            (Value::StructInstance { name, .. }, Type::Instance(n)) => name == n,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn interpret(source: &str) -> Result<(), String> {
        let tokens = Lexer::new(source).tokenize();
        let stmts = Parser::new(tokens).parse()?;
        let mut interp = Interpreter::new();
        interp.interpret(&stmts)
    }

    fn interpret_with_globals(source: &str) -> Result<Vec<Value>, String> {
        let tokens = Lexer::new(source).tokenize();
        let stmts = Parser::new(tokens).parse()?;
        let mut interp = Interpreter::new();
        interp.interpret(&stmts)?;
        Ok(interp.globals.into_values().collect())
    }

    #[test]
    fn test_simple_let() {
        interpret("cell x = 42").unwrap();
    }

    #[test]
    fn test_arithmetic() {
        interpret("cell x = 2 + 3 * 4").unwrap();
    }

    #[test]
    fn test_string_concat() {
        interpret(r#"cell s = "Hello, " + "World!""#).unwrap();
    }

    #[test]
    fn test_comparison() {
        interpret("cell _ = 5 > 3\ncell _ = 5 <= 5").unwrap();
    }

    #[test]
    fn test_string_comparison() {
        interpret(r#"cell _ = "abc" < "xyz""#).unwrap();
    }

    #[test]
    fn test_modulo() {
        interpret("cell x = 17 % 5").unwrap();
    }

    #[test]
    fn test_list_ops() {
        interpret("cell arr = [1, 2, 3]\ncell x = arr[0]").unwrap();
    }

    #[test]
    fn test_dict_ops() {
        interpret("cell d = {\"a\": 1, \"b\": 2}\ncell v = d[\"a\"]").unwrap();
    }

    #[test]
    fn test_if_else() {
        interpret("cell x = 0\nwhen yes\n    x = 1\nelse\n    x = 2").unwrap();
    }

    #[test]
    fn test_while_loop() {
        interpret("cell i = 0\nwhile i < 5\n    i = i + 1").unwrap();
    }

    #[test]
fn test_for_range() {
    interpret("cell s = 0\nover i in 0..10\n    s = s + i").unwrap();
}

    #[test]
fn test_for_list() {
    interpret("cell s = \"\"\nover c in [\"a\", \"b\"]\n    s = s + c").unwrap();
}

    #[test]
fn test_for_string() {
    interpret("cell s = \"\"\nover ch in \"abc\"\n    s = s + ch").unwrap();
}

    #[test]
    fn test_fn_call() {
        interpret("~add(a, b)\n    emit a + b\ncell x = add(2, 3)").unwrap();
    }

    #[test]
    fn test_recursion() {
        interpret(
            "~fib(n)\n    when n <= 1\n        emit n\n    fib(n-1) + fib(n-2)\ncell x = fib(5)",
        )
        .unwrap();
    }

    #[test]
    fn test_closure() {
        interpret(
            "~make_adder(x)\n    emit \\(y) y + x\ncell add5 = make_adder(5)\ncell x = add5(3)",
        )
        .unwrap();
    }

    #[test]
    fn test_match() {
        interpret(
            "cell x = 3\npick x\n    1 ->\n        x = 10\n    3 ->\n        x = 30\n    _ ->\n        x = 0",
        )
        .unwrap();
    }

    #[test]
    fn test_try_catch() {
        interpret("dare\n    assert(no)\ncatch e\n    shout(e)").unwrap();
    }

    #[test]
    fn test_builtins() {
        interpret("cell l = len([1, 2, 3])\ncell s = str(42)").unwrap();
    }

    #[test]
    fn test_string_builtins() {
        interpret("cell s = upper(\"hello\")\ncell t = trim(\"  x  \")").unwrap();
    }

    #[test]
    fn test_list_builtins() {
        interpret("cell arr = [3, 1, 2]\nsort(arr)\nreverse(arr)\npush(arr, 4)").unwrap();
    }

    #[test]
    fn test_math_builtins() {
        interpret("cell x = floor(3.7)\ncell y = max(10, 20)\ncell z = pow(2, 3)").unwrap();
    }

    #[test]
    fn test_json_builtins() {
        interpret("cell d = json_decode(\"[1, 2, 3]\") assert(d[1] == 2)").unwrap();
        interpret("cell s = json_encode(42) assert(s == \"42\")").unwrap();
        interpret("cell v = json_decode(\"null\") assert(v == none)").unwrap();
        interpret("cell d = json_decode(\"[1]\") assert(d[0] == 1)").unwrap();
        interpret("cell s = json_encode(\"hello\")").unwrap();
    }

    #[test]
    fn test_json_escapes() {
        interpret("cell d = json_decode(\"[1, 2]\") assert(d[0] == 1) assert(d[1] == 2)").unwrap();
    }

    #[test]
    fn test_json_validate() {
        interpret("cell v = json_validate(\"true\") assert(v)").unwrap();
        interpret("cell b = json_validate(\"bad\") assert(!b)").unwrap();
        interpret("cell b = json_validate(\"\\{\\\"a\\\":1\\}\") assert(b)").unwrap();
    }

    #[test]
    fn test_json_module() {
        interpret("load \"std/json\" as json\ncell s = json.encode(42) assert(s == \"42\")").unwrap();
        interpret("load \"std/json\" as json\ncell d = json.decode(\"[1, 2, 3]\") assert(d[0] == 1)").unwrap();
        interpret("load \"std/json\" as json\ncell p = json.pretty({\"a\": [1]}) assert(p != \"\")").unwrap();
        interpret("load \"std/json\" as json\nassert(json.validate(\"true\"))").unwrap();
        interpret("load \"std/json\" as json\nassert(!json.validate(\"bad\"))").unwrap();
    }

    #[test]
    fn test_string_interp_simple() {
        interpret(
            "cell name = \"world\"\ncell s = \"Hello {name}!\"\nassert(s == \"Hello world!\")",
        )
        .unwrap();
    }

    #[test]
    fn test_string_interp_expr() {
        interpret("cell s = \"2 + 3 = {2 + 3}\"\nassert(s == \"2 + 3 = 5\")").unwrap();
    }

    #[test]
    fn test_string_interp_literal_braces() {
        interpret("cell s = \"\\{hello\\}\"\nassert(s == \"\\{hello\\}\")").unwrap();
    }

    #[test]
    fn test_clock() {
        interpret("cell t = clock()").unwrap();
    }

    #[test]
    fn test_compound_assign() {
        interpret("cell x = 5\nx += 3\nx *= 2").unwrap();
    }

    #[test]
    fn test_modulo_assign() {
        interpret("cell x = 17\nx %= 5").unwrap();
    }

    #[test]
    fn test_nil() {
        interpret("cell x = none").unwrap();
    }

    #[test]
    fn test_nested_blocks() {
        interpret("cell x = 0\nwhen yes\n    cell y = 1\n    x = y").unwrap();
    }

    #[test]
    fn test_break_continue() {
        interpret(
            "cell i = 0\nwhile i < 10\n    i = i + 1\n    when i == 3\n        continue\n    when i == 5\n        break",
        )
        .unwrap();
    }

    #[test]
    fn test_range_iter() {
        interpret("cell s = 0\nover i in 0..5\n    s = s + i").unwrap();
    }

    #[test]
    fn test_with_globals() {
        let globals = interpret_with_globals("cell x = 10\n~f()\n    5");
        assert!(globals.is_ok());
    }

    // --- slicing tests ---

    #[test]
    fn test_slice_list_basic() {
        interpret("cell a = [0, 1, 2, 3, 4]\ncell x = a[1:3]\nassert(x == [1, 2])").unwrap();
    }

    #[test]
    fn test_slice_list_start_only() {
        interpret("cell a = [0, 1, 2, 3, 4]\ncell x = a[2:]\nassert(x == [2, 3, 4])").unwrap();
    }

    #[test]
    fn test_slice_list_end_only() {
        interpret("cell a = [0, 1, 2, 3, 4]\ncell x = a[:3]\nassert(x == [0, 1, 2])").unwrap();
    }

    #[test]
    fn test_slice_list_full() {
        interpret("cell a = [0, 1, 2, 3, 4]\ncell x = a[:]\nassert(x == [0, 1, 2, 3, 4])").unwrap();
    }

    #[test]
    fn test_slice_list_out_of_bounds() {
        interpret("cell a = [0, 1]\ncell x = a[5:10]").unwrap();
    }

    #[test]
    fn test_slice_string_basic() {
        interpret("cell s = \"hello\"\ncell x = s[1:4]\nassert(x == \"ell\")").unwrap();
    }

    #[test]
    fn test_slice_string_start_only() {
        interpret("cell s = \"hello\"\ncell x = s[2:]\nassert(x == \"llo\")").unwrap();
    }

    #[test]
    fn test_slice_string_end_only() {
        interpret("cell s = \"hello\"\ncell x = s[:4]\nassert(x == \"hell\")").unwrap();
    }

    #[test]
    fn test_slice_string_full() {
        interpret("cell s = \"hello\"\ncell x = s[:]\nassert(x == \"hello\")").unwrap();
    }

    // --- spread tests ---

    #[test]
    fn test_spread_in_list() {
        interpret("cell a = [1, 2]\ncell x = [0, ...a, 3]\nassert(x == [0, 1, 2, 3])").unwrap();
    }

    #[test]
    fn test_spread_in_call() {
        interpret(
            "~sum(a, b, c)\n    emit a + b + c\ncell items = [10, 20, 30]\ncell x = sum(...items)\nassert(x == 60)"
        ).unwrap();
    }

    #[test]
    fn test_spread_in_set() {
        interpret("cell a = [1, 2]\ncell x = {0, ...a, 3}").unwrap();
    }

    #[test]
    fn test_spread_empty_list() {
        interpret("cell a = []\ncell x = [1, ...a, 2]\nassert(x == [1, 2])").unwrap();
    }

    #[test]
    fn test_spread_error_standalone() {
        let result = interpret("...x");
        assert!(result.is_err());
    }

    #[test]
    fn test_raw_string() {
        interpret("cell s = r\"hello\\nworld\"\nassert(s == \"hello\\\\nworld\")").unwrap();
    }

    // --- destructuring tests ---

    #[test]
    fn test_destructure_list() {
        interpret("cell (a, b) = [10, 20]\nassert(a == 10)\nassert(b == 20)").unwrap();
    }

    #[test]
    fn test_destructure_list_rest() {
        interpret("cell (a, ...rest) = [1, 2, 3, 4]\nassert(a == 1)\nassert(rest == [2, 3, 4])").unwrap();
    }

    #[test]
    fn test_destructure_list_rest_empty() {
        interpret("cell (a, ...rest) = [1]\nassert(a == 1)\nassert(rest == [])").unwrap();
    }

    #[test]
    fn test_destructure_struct() {
        interpret(
            "shape Point\n    x: float\n    y: float\ncell p = Point(10, 20)\ncell {x, y} = p\nassert(x == 10)\nassert(y == 20)"
        ).unwrap();
    }

    #[test]
    fn test_destructure_dict() {
        interpret(
            "cell d = {\"x\": 100, \"y\": 200}\ncell {x, y} = d\nassert(x == 100)\nassert(y == 200)"
        ).unwrap();
    }

    #[test]
    fn test_destructure_list_error_not_list() {
        let r = interpret("cell (a, b) = 42");
        assert!(r.is_err());
    }

    #[test]
    fn test_debugger_breakpoint() {
        use super::debugger::{Debugger, DebugMode};
        let mut dbg = Debugger::new();
        let stmt = Stmt::Expr { span: Some(SourceSpan { line: 5, col: 0 }), expr: Box::new(Expr::Int(1)) };
        // No breakpoints -> should not pause
        assert!(!dbg.before_stmt(&stmt, "test.eltr"));
        assert_eq!(dbg.mode, DebugMode::Running);

        // Set breakpoint at line 5 -> should trigger
        dbg.breakpoints.push(super::debugger::Breakpoint {
            file: "test.eltr".to_string(),
            line: 5,
            enabled: true,
        });
        // Running mode with breakpoint hit returns true but stays Running
        assert!(dbg.before_stmt(&stmt, "test.eltr"));
        assert_eq!(dbg.mode, DebugMode::Running);

        // Different line -> no trigger
        let stmt2 = Stmt::Expr { span: Some(SourceSpan { line: 6, col: 0 }), expr: Box::new(Expr::Int(1)) };
        assert!(!dbg.before_stmt(&stmt2, "test.eltr"));

        // Different file -> no trigger
        assert!(!dbg.before_stmt(&stmt, "other.eltr"));
    }

    #[test]
    fn test_debugger_step_over() {
        use super::debugger::{Debugger, DebugMode};
        let mut dbg = Debugger::new();
        dbg.call_depth = 5;
        dbg.step_depth = 5;
        dbg.mode = DebugMode::StepOver;
        let stmt = Stmt::Expr { span: Some(SourceSpan { line: 3, col: 0 }), expr: Box::new(Expr::Int(1)) };

        // Same depth -> pauses
        assert!(dbg.before_stmt(&stmt, "test.eltr"));
        assert_eq!(dbg.mode, DebugMode::Paused);

        // Reset
        dbg.mode = DebugMode::StepOver;
        dbg.step_depth = 5;
        dbg.call_depth = 6; // deeper inside a function call

        // Deeper -> should not pause
        assert!(!dbg.before_stmt(&stmt, "test.eltr"));
    }

    #[test]
    fn test_debugger_step_out() {
        use super::debugger::{Debugger, DebugMode};
        let mut dbg = Debugger::new();
        dbg.call_depth = 5;
        dbg.step_depth = 5;
        dbg.mode = DebugMode::StepOut;

        let stmt = Stmt::Expr { span: Some(SourceSpan { line: 3, col: 0 }), expr: Box::new(Expr::Int(1)) };

        // Same depth -> should not pause (need to return first)
        assert!(!dbg.before_stmt(&stmt, "test.eltr"));

        // After return (depth decreased)
        dbg.call_depth = 4;
        assert!(dbg.before_stmt(&stmt, "test.eltr"));
        assert_eq!(dbg.mode, DebugMode::Paused);
    }

    // --- macro tests ---

    #[test]
    fn test_macro_stmt_block() {
        interpret(
            "cell x = 10
macro ensure_positive($v)
    when $v < 0
        x = 0
    else
        x = $v
ensure_positive(42)
assert(x == 42)"
        ).unwrap();
    }

    #[test]
    fn test_macro_with_cond() {
        interpret(
            "macro assert_true($x)
                when !($x)
                    shout \"assertion failed\"
cell ok = yes
assert_true(ok)"
        ).unwrap();
    }

    #[test]
    fn test_macro_if_else() {
        interpret(
            "macro unless($cond, $body)
                when !($cond)
                    $body
cell triggered = no
unless(yes == no, triggered = yes)
assert(triggered)"
        ).unwrap();
    }

    #[test]
    fn test_macro_no_args() {
        interpret(
            "macro greet()
                shout \"hi\"
greet()"
        ).unwrap();
    }

    #[test]
    fn test_macro_empty_body() {
        // Macro body with just `none` (effectively empty)
        interpret(
            "macro noop()
    none
cell x = 42
noop()
assert(x == 42)"
        ).unwrap();
    }

    #[test]
    fn test_macro_wrong_arg_count() {
        let result = interpret(
            "macro foo($x)
                $x
foo(1, 2)"
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_macro_dollar_ident() {
        // $ alone should be a valid identifier
        interpret("cell $x = 42\nassert($x == 42)").unwrap();
    }

    #[test]
    fn test_macro_dollar_in_expr() {
        interpret(
            "cell $value = 100
macro check($v)
    when $v != 100
        shout \"wrong\"
check($value)"
        ).unwrap();
    }
}
