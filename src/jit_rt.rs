use std::ffi::c_void;
use crate::interpreter::gc::{gc_heap, gc_heap_mut, GcData};

use crate::ast::{Expr, Stmt};
use crate::interpreter::value::Flow;
use crate::interpreter::{Interpreter, IterKind, Value, JIT_INTERP};

fn alloc_val(val: Value) -> *mut Value {
    Box::into_raw(Box::new(val))
}

unsafe fn read_val(ptr: *mut Value) -> Value {
    unsafe { (*ptr).clone() }
}

fn jit_number_op<F: Fn(f64, f64) -> Result<Value, String>>(
    a: *mut Value,
    b: *mut Value,
    op: F,
) -> *mut Value {
    let a_val = unsafe { read_val(a) };
    let b_val = unsafe { read_val(b) };
    match (a_val.as_float(), b_val.as_float()) {
        (Some(va), Some(vb)) => match op(va, vb) {
            Ok(v) => alloc_val(v),
            Err(e) => alloc_val(Value::String(e)),
        },
        _ => alloc_val(Value::String("Type error: expected numbers".into())),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_number(n: f64) -> *mut Value {
    alloc_val(Value::Float(n))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_bool(b: bool) -> *mut Value {
    alloc_val(Value::Boolean(b))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_nil() -> *mut Value {
    alloc_val(Value::Nil)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_add(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    let result = match (&av, &bv) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
        (Value::Int(a), Value::Float(b)) => Value::Float(*a as f64 + b),
        (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
        (Value::String(a), Value::String(b)) => Value::String(format!("{}{}", a, b)),
        (Value::String(a), b) => Value::String(format!("{}{}", a, b)),
        (a, Value::String(b)) => Value::String(format!("{}{}", a, b)),
        (Value::List(a), Value::List(b)) => {
            let mut m = match gc_heap().unwrap().get(*a) {
                GcData::List(items) => items.clone(),
                _ => unreachable!(),
            };
            let b_items = match gc_heap().unwrap().get(*b) {
                GcData::List(items) => items.clone(),
                _ => unreachable!(),
            };
            m.extend(b_items);
            Value::List(gc_heap_mut().unwrap().alloc(GcData::List(m)))
        }
        _ => Value::String("Cannot add values".into()),
    };
    alloc_val(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_sub(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_number_op(a, b, |a, b| Ok(Value::Float(a - b)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_mul(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_number_op(a, b, |a, b| Ok(Value::Float(a * b)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_div(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_number_op(a, b, |a, b| {
        if b == 0.0 {
            Err("Division by zero".into())
        } else {
            Ok(Value::Float(a / b))
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_mod(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_number_op(a, b, |a, b| {
        if b == 0.0 {
            Err("Modulo by zero".into())
        } else {
            Ok(Value::Float(a % b))
        }
    })
}

fn jit_int_op(a: *mut Value, b: *mut Value, f: fn(i64, i64) -> Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    match (av.as_float(), bv.as_float()) {
        (Some(a), Some(b)) => alloc_val(f(a as i64, b as i64)),
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_bit_and(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_int_op(a, b, |a, b| Value::Int(a & b))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_bit_or(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_int_op(a, b, |a, b| Value::Int(a | b))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_bit_xor(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_int_op(a, b, |a, b| Value::Int(a ^ b))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_shl(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_int_op(a, b, |a, b| Value::Int(a << b))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_shr(a: *mut Value, b: *mut Value) -> *mut Value {
    jit_int_op(a, b, |a, b| Value::Int(a >> b))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_bit_not(a: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    match av {
        Value::Int(n) => alloc_val(Value::Int(!n)),
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_eq(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    alloc_val(Value::Boolean(av == bv))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_neq(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    alloc_val(Value::Boolean(av != bv))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_lt(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    let result = match (av.as_float(), bv.as_float()) {
        (Some(a), Some(b)) => Value::Boolean(a < b),
        _ => match (&av, &bv) {
            (Value::String(a), Value::String(b)) => Value::Boolean(a < b),
            _ => Value::Boolean(false),
        },
    };
    alloc_val(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_le(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    let result = match (av.as_float(), bv.as_float()) {
        (Some(a), Some(b)) => Value::Boolean(a <= b),
        _ => match (&av, &bv) {
            (Value::String(a), Value::String(b)) => Value::Boolean(a <= b),
            _ => Value::Boolean(false),
        },
    };
    alloc_val(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_gt(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    let result = match (av.as_float(), bv.as_float()) {
        (Some(a), Some(b)) => Value::Boolean(a > b),
        _ => match (&av, &bv) {
            (Value::String(a), Value::String(b)) => Value::Boolean(a > b),
            _ => Value::Boolean(false),
        },
    };
    alloc_val(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_ge(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    let bv = unsafe { read_val(b) };
    let result = match (av.as_float(), bv.as_float()) {
        (Some(a), Some(b)) => Value::Boolean(a >= b),
        _ => match (&av, &bv) {
            (Value::String(a), Value::String(b)) => Value::Boolean(a >= b),
            _ => Value::Boolean(false),
        },
    };
    alloc_val(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_negate(a: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    match av.as_float() {
        Some(n) => alloc_val(Value::Float(-n)),
        None => alloc_val(Value::String("Cannot negate non-number".into())),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_not(a: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    alloc_val(Value::Boolean(!av.is_truthy()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_and(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    if !av.is_truthy() {
        return alloc_val(Value::Boolean(false));
    }
    let bv = unsafe { read_val(b) };
    alloc_val(Value::Boolean(bv.is_truthy()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_or(a: *mut Value, b: *mut Value) -> *mut Value {
    let av = unsafe { read_val(a) };
    if av.is_truthy() {
        return alloc_val(Value::Boolean(true));
    }
    let bv = unsafe { read_val(b) };
    alloc_val(Value::Boolean(bv.is_truthy()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_range(a: *mut Value, b: *mut Value) -> *mut Value {
    let start = unsafe { read_val(a) };
    let end = unsafe { read_val(b) };
    match (start, end) {
        (Value::Float(s), Value::Float(e)) => alloc_val(Value::Range(s, e)),
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_truthy(v: *mut Value) -> u8 {
    let v = unsafe { read_val(v) };
    v.is_truthy() as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_is_nil(v: *mut Value) -> u8 {
    let v = unsafe { read_val(v) };
    matches!(v, Value::Nil) as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_make_iter(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::List(h) => alloc_val(Value::Iterator(IterKind::List { handle: h, index: 0 })),
        Value::Set(h) => alloc_val(Value::Iterator(IterKind::List { handle: h, index: 0 })),
        Value::String(s) => alloc_val(Value::Iterator(IterKind::String {
            chars: s.chars().map(|c| c.to_string()).collect(),
            index: 0,
        })),
        Value::Range(a, b) => alloc_val(Value::Iterator(IterKind::Range {
            start: a,
            end: b,
            current: a,
        })),
        val => alloc_val(val),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_print(v: *mut Value) -> *mut Value {
    let val = unsafe { read_val(v) };
    print!("{}", val);
    alloc_val(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_println(v: *mut Value) -> *mut Value {
    let val = unsafe { read_val(v) };
    println!("{}", val);
    alloc_val(val)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_string_from_data(data: *const u8, len: usize) -> *mut Value {
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(data, len)) };
    alloc_val(Value::String(s.to_string()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_list(items: *mut *mut Value, count: usize) -> *mut Value {
    let items_slice = unsafe { std::slice::from_raw_parts(items, count) };
    let list: Vec<Value> = items_slice
        .iter()
        .map(|&p| unsafe { read_val(p) })
        .collect();
    alloc_val(Value::List(gc_heap_mut().unwrap().alloc(GcData::List(list))))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_dict() -> *mut Value {
    alloc_val(Value::Dict(gc_heap_mut().unwrap().alloc(GcData::Dict(Vec::new()))))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_dict_set(dict: *mut Value, key: *mut Value, val: *mut Value) -> *mut Value {
    unsafe {
        match &mut *dict {
            Value::Dict(h) => {
                let entries = match gc_heap_mut().unwrap().get_mut(*h) {
                    GcData::Dict(entries) => entries,
                    _ => unreachable!(),
                };
                let k = read_val(key);
                let v = read_val(val);
                if let Some(pos) = entries.iter().position(|(ek, _)| *ek == k) {
                    entries[pos] = (k, v);
                } else {
                    entries.push((k, v));
                }
            }
            _ => {}
        }
    }
    dict
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_index_get(obj: *mut Value, index: *mut Value) -> *mut Value {
    let o = unsafe { read_val(obj) };
    let i = unsafe { read_val(index) };
    match (&o, &i) {
        (Value::List(h), Value::Float(idx)) => {
            let items = match gc_heap().unwrap().get(*h) {
                GcData::List(items) => items,
                _ => unreachable!(),
            };
            let idx = *idx as usize;
            if idx < items.len() {
                alloc_val(items[idx].clone())
            } else {
                alloc_val(Value::Nil)
            }
        }
        (Value::String(s), Value::Float(idx)) => {
            let idx = *idx as usize;
            if let Some(c) = s.chars().nth(idx) {
                alloc_val(Value::String(c.to_string()))
            } else {
                alloc_val(Value::Nil)
            }
        }
        (Value::Dict(h), _) => {
            let entries = match gc_heap().unwrap().get(*h) {
                GcData::Dict(entries) => entries,
                _ => unreachable!(),
            };
            if let Some(pos) = entries.iter().position(|(k, _)| *k == i) {
                alloc_val(entries[pos].1.clone())
            } else {
                alloc_val(Value::Nil)
            }
        }
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_call(
    interp: *mut c_void,
    name: *const u8,
    name_len: usize,
    args: *mut *mut Value,
    arg_count: usize,
) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name, name_len)) };
    let args_slice = unsafe { std::slice::from_raw_parts(args, arg_count) };
    let args: Vec<Value> = args_slice.iter().map(|&p| unsafe { read_val(p) }).collect();
    match interp.jit_call(name, args) {
        Ok(val) => alloc_val(val),
        Err(e) => alloc_val(Value::String(format!("JIT call error: {}", e))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_method_call(
    interp: *mut c_void,
    obj: *mut Value,
    name: *const u8,
    name_len: usize,
    args: *mut *mut Value,
    arg_count: usize,
) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let method =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name, name_len)) };
    let args_slice = unsafe { std::slice::from_raw_parts(args, arg_count) };
    let obj_val = unsafe { read_val(obj) };
    let mut all_args = vec![obj_val.clone()];
    for &p in args_slice.iter() {
        all_args.push(unsafe { read_val(p) });
    }
    match interp.jit_method_call(method, obj_val, all_args) {
        Ok(val) => alloc_val(val),
        Err(e) => alloc_val(Value::String(format!("JIT method error: {}", e))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_iter_next(iter: *mut Value) -> *mut Value {
    let v = unsafe { &mut *iter };
    match v {
        Value::Iterator(kind) => {
            let result = JIT_INTERP.with(|cell| {
                if let Some(ptr) = cell.get() {
                    let interp = unsafe { &mut *ptr };
                    interp.iter_next(kind)
                } else {
                    Value::Nil
                }
            });
            alloc_val(result)
        }
        _ => alloc_val(Value::Nil),
    }
}

pub fn jit_store_stmt(stmt: Stmt) -> usize {
    Box::into_raw(Box::new(stmt)) as usize
}

pub fn jit_store_expr(expr: Expr) -> usize {
    Box::into_raw(Box::new(expr)) as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_in(left: *mut Value, right: *mut Value) -> *mut Value {
    let l = unsafe { read_val(left) };
    let r = unsafe { read_val(right) };
    let result = match &r {
        Value::List(h) => match gc_heap().unwrap().get(*h) {
            GcData::List(items) => items.contains(&l),
            _ => unreachable!(),
        },
        Value::Set(h) => match gc_heap().unwrap().get(*h) {
            GcData::Set(items) => items.contains(&l),
            _ => unreachable!(),
        },
        Value::Dict(h) => match gc_heap().unwrap().get(*h) {
            GcData::Dict(entries) => entries.iter().any(|(k, _)| *k == l),
            _ => unreachable!(),
        },
        Value::String(s) => {
            if let Value::String(pat) = &l {
                s.contains(pat)
            } else {
                false
            }
        }
        _ => false,
    };
    alloc_val(Value::Boolean(result))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_field_get(
    interp: *mut c_void,
    obj: *mut Value,
    field: *const u8,
    field_len: usize,
) -> *mut Value {
    let _interp = unsafe { &mut *(interp as *mut Interpreter) };
    let val = unsafe { read_val(obj) };
    let field_name =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(field, field_len)) };
    match &val {
        Value::StructInstance { name: _, fields: h } => {
            let fields = match gc_heap().unwrap().get(*h) {
                GcData::StructFields(fields) => fields,
                _ => unreachable!(),
            };
            if let Some(v) = fields.get(field_name) {
                alloc_val(v.clone())
            } else {
                alloc_val(Value::Nil)
            }
        }
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_field_set(
    interp: *mut c_void,
    obj: *mut Value,
    field: *const u8,
    field_len: usize,
    val: *mut Value,
) -> *mut Value {
    let _interp = unsafe { &mut *(interp as *mut Interpreter) };
    let obj_val = unsafe { &mut *obj };
    let field_name =
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(field, field_len)) };
    let v = unsafe { read_val(val) };
    match obj_val {
        Value::StructInstance { fields: h, .. } | Value::ClassInstance { fields: h, .. } => {
            let f = match gc_heap_mut().unwrap().get_mut(*h) {
                GcData::StructFields(fields) | GcData::ClassFields(fields) => fields,
                _ => unreachable!(),
            };
            f.insert(field_name.to_string(), v);
        }
        _ => {}
    }
    alloc_val(Value::Nil)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_exec_match(
    interp: *mut c_void,
    value: *mut Value,
    stmt_addr: usize,
) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let val = unsafe { read_val(value) };
    let stmt = unsafe { &*(stmt_addr as *const Stmt) };
    if let Stmt::Match { span: _, value: _, arms } = stmt {
        match interp.jit_exec_match(&val, arms) {
            Ok(Flow::Return(v)) => alloc_val(v),
            _ => alloc_val(Value::Nil),
        }
    } else {
        alloc_val(Value::Nil)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_exec_try(interp: *mut c_void, stmt_addr: usize) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let stmt = unsafe { &*(stmt_addr as *const Stmt) };
    if let Stmt::Try {
        span: _,
        body,
        catch_var,
        catch_body,
    } = stmt
    {
        match interp.jit_exec_try(body, catch_var, catch_body) {
            Ok(Flow::Return(v)) => alloc_val(v),
            _ => alloc_val(Value::Nil),
        }
    } else {
        alloc_val(Value::Nil)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_get_global(
    interp: *mut c_void,
    name: *const u8,
    name_len: usize,
) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name, name_len)) };
    match interp.jit_get_global(name) {
        Ok(v) => alloc_val(v),
        Err(e) => alloc_val(Value::String(e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_set_global(
    interp: *mut c_void,
    name: *const u8,
    name_len: usize,
    val: *mut Value,
) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name, name_len)) };
    let v = unsafe { read_val(val) };
    match interp.jit_set_global(name, v) {
        Ok(()) => alloc_val(Value::Nil),
        Err(e) => alloc_val(Value::String(e)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_exec_nested_stmt(interp: *mut c_void, stmt_addr: usize) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let stmt = unsafe { &*(stmt_addr as *const Stmt) };
    match interp.jit_exec_nested_stmt(stmt) {
        Ok(Flow::Return(v)) => alloc_val(v),
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_await(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::Future(shared) => {
            let mut guard = shared.mutex.lock().unwrap();
            while guard.is_none() {
                guard = shared.cvar.wait(guard).unwrap();
            }
            match guard.take().unwrap() {
                Ok(v) => alloc_val(v),
                Err(e) => alloc_val(Value::String(e)),
            }
        }
        other => alloc_val(Value::String(format!("Cannot await '{}'", other.type_name()))),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_yield(interp: *mut c_void, val: *mut Value) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let val = unsafe { read_val(val) };
    if let Some(ref tx) = interp.generator_channel {
        tx.send(val).ok();
    }
    alloc_val(Value::Nil)
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_make_closure(interp: *mut c_void, expr_addr: usize) -> *mut Value {
    let interp = unsafe { &mut *(interp as *mut Interpreter) };
    let expr = unsafe { &*(expr_addr as *const Expr) };
    if let Expr::Fn {
        generic_params,
        params,
        body,
    } = expr
    {
        let val = interp.jit_make_closure(params, body, generic_params);
        alloc_val(val)
    } else {
        alloc_val(Value::Nil)
    }
}

// ---- Result/Option runtime support ----

#[unsafe(no_mangle)]
pub extern "C" fn __jit_ok(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    alloc_val(Value::Ok(Box::new(v)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_err(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    alloc_val(Value::Err(Box::new(v)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_some(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    alloc_val(Value::Some(Box::new(v)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_is_ok(val: *mut Value) -> u8 {
    let v = unsafe { read_val(val) };
    matches!(v, Value::Ok(_)) as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_is_err(val: *mut Value) -> u8 {
    let v = unsafe { read_val(val) };
    matches!(v, Value::Err(_)) as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_is_some(val: *mut Value) -> u8 {
    let v = unsafe { read_val(val) };
    matches!(v, Value::Some(_)) as u8
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_unwrap_ok(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::Ok(inner) => alloc_val(*inner),
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_unwrap_err(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::Err(inner) => alloc_val(*inner),
        _ => alloc_val(Value::Nil),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __jit_unwrap_some(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::Some(inner) => alloc_val(*inner),
        _ => alloc_val(Value::Nil),
    }
}

// ---- Set literal support ----

#[unsafe(no_mangle)]
pub extern "C" fn __jit_set(items: *mut *mut Value, count: usize) -> *mut Value {
    let items_slice = unsafe { std::slice::from_raw_parts(items, count) };
    let list: Vec<Value> = items_slice
        .iter()
        .map(|&p| unsafe { read_val(p) })
        .collect();
    alloc_val(Value::Set(gc_heap_mut().unwrap().alloc(GcData::Set(list))))
}

// ---- Slice support ----

#[unsafe(no_mangle)]
pub extern "C" fn __jit_slice(
    obj: *mut Value,
    start: *mut Value,
    end: *mut Value,
) -> *mut Value {
    let o = unsafe { read_val(obj) };
    let sv = unsafe { read_val(start) };
    let ev = unsafe { read_val(end) };
    let start_idx = match sv.as_float() {
        Some(n) if n >= 0.0 => Some(n as usize),
        _ => None,
    };
    let end_idx = match ev.as_float() {
        Some(n) if n >= 0.0 => Some(n as usize),
        _ => None,
    };
    match &o {
        Value::List(h) => {
            let items = match gc_heap().unwrap().get(*h) {
                GcData::List(items) => items,
                _ => unreachable!(),
            };
            let s = start_idx.unwrap_or(0);
            let e = end_idx.unwrap_or(items.len());
            let e = e.min(items.len());
            let sliced: Vec<Value> = items[s..e].to_vec();
            alloc_val(Value::List(gc_heap_mut().unwrap().alloc(GcData::List(sliced))))
        }
        Value::String(st) => {
            let chars: Vec<char> = st.chars().collect();
            let s = start_idx.unwrap_or(0);
            let e = end_idx.unwrap_or(chars.len());
            let e = e.min(chars.len());
            let sliced: String = chars[s..e].iter().collect();
            alloc_val(Value::String(sliced))
        }
        _ => alloc_val(Value::Nil),
    }
}

// ---- Tuple support ----

#[unsafe(no_mangle)]
pub extern "C" fn __jit_tuple(items: *mut *mut Value, count: usize) -> *mut Value {
    let items_slice = unsafe { std::slice::from_raw_parts(items, count) };
    let list: Vec<Value> = items_slice
        .iter()
        .map(|&p| unsafe { read_val(p) })
        .collect();
    alloc_val(Value::Tuple(list))
}

// ---- Spread support ----

#[unsafe(no_mangle)]
pub extern "C" fn __jit_spread(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::List(h) => alloc_val(Value::List(h)),
        Value::Set(h) => alloc_val(Value::List(h)),
        other => alloc_val(other),
    }
}

// ---- Try propagation (used by JIT for Expr::Try) ----
// Returns the unwrapped value on Ok/Some.
// On Err, returns a Value::String containing the error message (JIT limitation:
// we can't easily do control flow propagation from JIT, so errors become strings).
#[unsafe(no_mangle)]
pub extern "C" fn __jit_try_propagate(val: *mut Value) -> *mut Value {
    let v = unsafe { read_val(val) };
    match v {
        Value::Ok(inner) => alloc_val(*inner),
        Value::Some(inner) => alloc_val(*inner),
        Value::Err(e) => alloc_val(Value::String(format!("{}", e))),
        other => alloc_val(other),
    }
}
