use std::collections::{HashMap, HashSet};

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::ExecutionEngine;
use inkwell::module::Module;
use inkwell::types::IntType;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::OptimizationLevel;

use crate::ast::{BinaryOpKind, Expr, Stmt, UnaryOpKind};
use crate::interpreter::Value;

use std::ffi::c_void;

type JitFn = unsafe extern "C" fn(*mut c_void, *mut *mut Value, usize) -> *mut Value;

#[allow(dead_code)]
struct RuntimeFns<'ctx> {
    add: FunctionValue<'ctx>,
    sub: FunctionValue<'ctx>,
    mul: FunctionValue<'ctx>,
    div: FunctionValue<'ctx>,
    modulo: FunctionValue<'ctx>,
    eq: FunctionValue<'ctx>,
    neq: FunctionValue<'ctx>,
    lt: FunctionValue<'ctx>,
    le: FunctionValue<'ctx>,
    gt: FunctionValue<'ctx>,
    ge: FunctionValue<'ctx>,
    negate: FunctionValue<'ctx>,
    not: FunctionValue<'ctx>,
    and: FunctionValue<'ctx>,
    or: FunctionValue<'ctx>,
    number: FunctionValue<'ctx>,
    boolean: FunctionValue<'ctx>,
    nil: FunctionValue<'ctx>,
    truthy: FunctionValue<'ctx>,
    is_nil: FunctionValue<'ctx>,
    make_iter: FunctionValue<'ctx>,
    iter_next: FunctionValue<'ctx>,
    print: FunctionValue<'ctx>,
    println: FunctionValue<'ctx>,
    range: FunctionValue<'ctx>,
    string_from_data: FunctionValue<'ctx>,
    list: FunctionValue<'ctx>,
    dict: FunctionValue<'ctx>,
    dict_set: FunctionValue<'ctx>,
    bit_and: FunctionValue<'ctx>,
    bit_or: FunctionValue<'ctx>,
    bit_xor: FunctionValue<'ctx>,
    shl: FunctionValue<'ctx>,
    shr: FunctionValue<'ctx>,
    bit_not: FunctionValue<'ctx>,
    index_get: FunctionValue<'ctx>,
    call: FunctionValue<'ctx>,
    method_call: FunctionValue<'ctx>,
    in_op: FunctionValue<'ctx>,
    field_get: FunctionValue<'ctx>,
    field_set: FunctionValue<'ctx>,
    exec_match: FunctionValue<'ctx>,
    exec_try: FunctionValue<'ctx>,
    make_closure: FunctionValue<'ctx>,
    yield_fn: FunctionValue<'ctx>,
    await_fn: FunctionValue<'ctx>,
    get_global: FunctionValue<'ctx>,
    set_global: FunctionValue<'ctx>,
    exec_nested_stmt: FunctionValue<'ctx>,
    // Result/Option
    ok_fn: FunctionValue<'ctx>,
    err_fn: FunctionValue<'ctx>,
    some_fn: FunctionValue<'ctx>,
    is_ok: FunctionValue<'ctx>,
    is_err: FunctionValue<'ctx>,
    is_some: FunctionValue<'ctx>,
    unwrap_ok: FunctionValue<'ctx>,
    unwrap_err: FunctionValue<'ctx>,
    unwrap_some: FunctionValue<'ctx>,
    // Set/Slice/Spread
    set_fn: FunctionValue<'ctx>,
    slice_fn: FunctionValue<'ctx>,
    spread_fn: FunctionValue<'ctx>,
    // Tuple
    tuple_fn: FunctionValue<'ctx>,
    // Try propagation
    try_propagate: FunctionValue<'ctx>,
}

struct LoopContext<'ctx> {
    cond_block: BasicBlock<'ctx>,
    end_block: BasicBlock<'ctx>,
}

pub struct JitEngine {
    _context: &'static Context,
    engines: Vec<ExecutionEngine<'static>>,
    compiled: HashMap<String, JitFn>,
    failed: HashSet<String>,
}

impl JitEngine {
    pub fn new() -> Self {
        let context = Box::leak(Box::new(Context::create()));
        JitEngine {
            _context: context,
            engines: Vec::new(),
            compiled: HashMap::new(),
            failed: HashSet::new(),
        }
    }

    pub fn try_compile(
        &mut self,
        name: &str,
        params: &[(String, Option<crate::ast::Type>, Option<Expr>)],
        body: &[Stmt],
    ) -> Option<JitFn> {
        if let Some(&func) = self.compiled.get(name) {
            return Some(func);
        }
        if self.failed.contains(name) {
            return None;
        }
        match self.compile_impl(name, params, body) {
            Ok(func) => {
                self.compiled.insert(name.to_string(), func);
                Some(func)
            }
            Err(e) => {
                eprintln!("[jit] failed to compile '{}': {}", name, e);
                self.failed.insert(name.to_string());
                None
            }
        }
    }

    fn compile_impl(
        &mut self,
        name: &str,
        params: &[(String, Option<crate::ast::Type>, Option<Expr>)],
        body: &[Stmt],
    ) -> Result<JitFn, String> {
        let module = self._context.create_module(&format!("jit_{}", name));
        let builder = self._context.create_builder();
        let ctx = self._context;

        let ptr_type = ctx.ptr_type(inkwell::AddressSpace::default());
        let i64_type = ctx.i64_type();
        let f64_type = ctx.f64_type();
        let i8_type = ctx.i8_type();

        let truthy_type = i8_type.fn_type(&[ptr_type.into()], false);
        let unary_ptr_type = ptr_type.fn_type(&[ptr_type.into()], false);
        let call_type = ptr_type.fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                i64_type.into(),
                ptr_type.into(),
                i64_type.into(),
            ],
            false,
        );
        let method_call_type = ptr_type.fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                i64_type.into(),
                ptr_type.into(),
                i64_type.into(),
            ],
            false,
        );
        let binary_fn_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        let field_get_type = ptr_type.fn_type(
            &[
                ptr_type.into(),
                ptr_type.into(),
                ptr_type.into(),
                i64_type.into(),
            ],
            false,
        );
        let exec_match_type =
            ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), i64_type.into()], false);
        let exec_try_type = ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false);
        let make_closure_type = ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false);

        let rt = RuntimeFns {
            add: binary_rt(&module, "__jit_add", ptr_type),
            sub: binary_rt(&module, "__jit_sub", ptr_type),
            mul: binary_rt(&module, "__jit_mul", ptr_type),
            div: binary_rt(&module, "__jit_div", ptr_type),
            modulo: binary_rt(&module, "__jit_mod", ptr_type),
            eq: binary_rt(&module, "__jit_eq", ptr_type),
            neq: binary_rt(&module, "__jit_neq", ptr_type),
            lt: binary_rt(&module, "__jit_lt", ptr_type),
            le: binary_rt(&module, "__jit_le", ptr_type),
            gt: binary_rt(&module, "__jit_gt", ptr_type),
            ge: binary_rt(&module, "__jit_ge", ptr_type),
            negate: unary_ptr_rt(&module, "__jit_negate", ptr_type),
            not: unary_ptr_rt(&module, "__jit_not", ptr_type),
            and: binary_rt(&module, "__jit_and", ptr_type),
            or: binary_rt(&module, "__jit_or", ptr_type),
            number: module.add_function(
                "__jit_number",
                ptr_type.fn_type(&[f64_type.into()], false),
                None,
            ),
            boolean: module.add_function(
                "__jit_bool",
                ptr_type.fn_type(&[i8_type.into()], false),
                None,
            ),
            nil: module.add_function("__jit_nil", ptr_type.fn_type(&[], false), None),
            truthy: module.add_function("__jit_truthy", truthy_type, None),
            is_nil: module.add_function("__jit_is_nil", truthy_type, None),
            make_iter: module.add_function("__jit_make_iter", unary_ptr_type, None),
            iter_next: module.add_function("__jit_iter_next", unary_ptr_type, None),
            print: module.add_function("__jit_print", unary_ptr_type, None),
            println: module.add_function("__jit_println", unary_ptr_type, None),
            bit_and: binary_rt(&module, "__jit_bit_and", ptr_type),
            bit_or: binary_rt(&module, "__jit_bit_or", ptr_type),
            bit_xor: binary_rt(&module, "__jit_bit_xor", ptr_type),
            shl: binary_rt(&module, "__jit_shl", ptr_type),
            shr: binary_rt(&module, "__jit_shr", ptr_type),
            bit_not: unary_ptr_rt(&module, "__jit_bit_not", ptr_type),
            range: binary_rt(&module, "__jit_range", ptr_type),
            string_from_data: module.add_function(
                "__jit_string_from_data",
                ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false),
                None,
            ),
            list: module.add_function(
                "__jit_list",
                ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false),
                None,
            ),
            dict: module.add_function("__jit_dict", ptr_type.fn_type(&[], false), None),
            dict_set: module.add_function(
                "__jit_dict_set",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), ptr_type.into()], false),
                None,
            ),
            index_get: binary_rt(&module, "__jit_index_get", ptr_type),
            call: module.add_function("__jit_call", call_type, None),
            method_call: module.add_function("__jit_method_call", method_call_type, None),
            in_op: module.add_function("__jit_in", binary_fn_type, None),
            field_get: module.add_function("__jit_field_get", field_get_type, None),
            field_set: module.add_function(
                "__jit_field_set",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), ptr_type.into(), i64_type.into(), ptr_type.into()], false),
                None,
            ),
            exec_match: module.add_function("__jit_exec_match", exec_match_type, None),
            exec_try: module.add_function("__jit_exec_try", exec_try_type, None),
            make_closure: module.add_function("__jit_make_closure", make_closure_type, None),
            yield_fn: module.add_function(
                "__jit_yield",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false),
                None,
            ),
            await_fn: module.add_function("__jit_await", unary_ptr_type, None),
            get_global: module.add_function(
                "__jit_get_global",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), i64_type.into()], false),
                None,
            ),
            set_global: module.add_function(
                "__jit_set_global",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), i64_type.into(), ptr_type.into()], false),
                None,
            ),
            exec_nested_stmt: module.add_function("__jit_exec_nested_stmt", exec_try_type, None),
            ok_fn: module.add_function("__jit_ok", unary_ptr_type, None),
            err_fn: module.add_function("__jit_err", unary_ptr_type, None),
            some_fn: module.add_function("__jit_some", unary_ptr_type, None),
            is_ok: module.add_function("__jit_is_ok", truthy_type, None),
            is_err: module.add_function("__jit_is_err", truthy_type, None),
            is_some: module.add_function("__jit_is_some", truthy_type, None),
            unwrap_ok: module.add_function("__jit_unwrap_ok", unary_ptr_type, None),
            unwrap_err: module.add_function("__jit_unwrap_err", unary_ptr_type, None),
            unwrap_some: module.add_function("__jit_unwrap_some", unary_ptr_type, None),
            set_fn: module.add_function(
                "__jit_set",
                ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false),
                None,
            ),
            slice_fn: module.add_function(
                "__jit_slice",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), ptr_type.into()], false),
                None,
            ),
            spread_fn: module.add_function("__jit_spread", unary_ptr_type, None),
            tuple_fn: module.add_function(
                "__jit_tuple",
                ptr_type.fn_type(&[ptr_type.into(), i64_type.into()], false),
                None,
            ),
            try_propagate: module.add_function(
                "__jit_try_propagate",
                ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false),
                None,
            ),
        };

        let fn_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into(), i64_type.into()], false);
        let function = module.add_function(name, fn_type, None);
        let entry = ctx.append_basic_block(function, "entry");
        builder.position_at_end(entry);

        let interp_param = function
            .get_nth_param(0)
            .ok_or("missing interp")?
            .into_pointer_value();
        let args_ptr = function
            .get_nth_param(1)
            .ok_or("missing args")?
            .into_pointer_value();
        let _arg_count = function
            .get_nth_param(2)
            .ok_or("missing count")?
            .into_int_value();

        let interp_alloca = builder
            .build_alloca(ptr_type, "interp")
            .map_err(|e| format!("interp alloca: {:?}", e))?;
        builder
            .build_store(interp_alloca, interp_param)
            .map_err(|e| format!("store interp: {:?}", e))?;

        let mut locals: HashMap<String, PointerValue> = HashMap::new();
        for (i, (pn, _, _)) in params.iter().enumerate() {
            let alloca = builder
                .build_alloca(ptr_type, pn)
                .map_err(|e| format!("alloca: {:?}", e))?;
            let idx = i64_type.const_int(i as u64, false);
            let gep = unsafe {
                builder
                    .build_in_bounds_gep(ptr_type, args_ptr, &[idx], &format!("arg{}_ptr", i))
                    .map_err(|e| format!("gep: {:?}", e))?
            };
            let loaded = builder
                .build_load(ptr_type, gep, &format!("arg{}", i))
                .map_err(|e| format!("load: {:?}", e))?
                .into_pointer_value();
            builder
                .build_store(alloca, loaded)
                .map_err(|e| format!("store: {:?}", e))?;
            locals.insert(pn.clone(), alloca);
        }

        compile_fn_body(
            ctx,
            &builder,
            &rt,
            &ptr_type,
            &f64_type,
            &i8_type,
            body,
            &mut locals,
            function,
            None,
            interp_alloca,
        )?;

        let ee = module
            .create_jit_execution_engine(OptimizationLevel::Aggressive)
            .map_err(|e| format!("create_ee: {:?}", e))?;
        let jit_fn = unsafe {
            ee.get_function::<JitFn>(name)
                .map_err(|e| format!("get_function: {:?}", e))?
                .into_raw()
        };
        self.engines.push(ee);
        Ok(jit_fn)
    }
}

fn binary_rt<'ctx>(
    module: &Module<'ctx>,
    name: &str,
    pt: inkwell::types::PointerType<'ctx>,
) -> FunctionValue<'ctx> {
    module.add_function(name, pt.fn_type(&[pt.into(), pt.into()], false), None)
}

fn unary_ptr_rt<'ctx>(
    module: &Module<'ctx>,
    name: &str,
    pt: inkwell::types::PointerType<'ctx>,
) -> FunctionValue<'ctx> {
    module.add_function(name, pt.fn_type(&[pt.into()], false), None)
}

fn compile_fn_body<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    ptr_type: &inkwell::types::PointerType<'ctx>,
    f64_type: &inkwell::types::FloatType<'ctx>,
    i8_type: &IntType<'ctx>,
    body: &[Stmt],
    locals: &mut HashMap<String, PointerValue<'ctx>>,
    function: FunctionValue<'ctx>,
    loop_ctx: Option<&LoopContext<'ctx>>,
    interp_alloca: PointerValue<'ctx>,
) -> Result<(), String> {
    let last_val = compile_stmts(
        context,
        builder,
        rt,
        ptr_type,
        f64_type,
        i8_type,
        body,
        locals,
        function,
        loop_ctx,
        interp_alloca,
    )?;
    let ret_val = match last_val {
        Some(v) => v,
        None => {
            let call = builder
                .build_call(rt.nil, &[], "nil_ret")
                .map_err(|e| format!("nil call: {:?}", e))?;
            ptr_from_call(call)
        }
    };
    builder
        .build_return(Some(&ret_val as &dyn BasicValue))
        .map_err(|e| format!("final ret: {:?}", e))?;
    Ok(())
}

fn compile_stmts<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    ptr_type: &inkwell::types::PointerType<'ctx>,
    f64_type: &inkwell::types::FloatType<'ctx>,
    i8_type: &IntType<'ctx>,
    body: &[Stmt],
    locals: &mut HashMap<String, PointerValue<'ctx>>,
    function: FunctionValue<'ctx>,
    loop_ctx: Option<&LoopContext<'ctx>>,
    interp_alloca: PointerValue<'ctx>,
) -> Result<Option<PointerValue<'ctx>>, String> {
    let mut last_val: Option<PointerValue<'ctx>> = None;

    for stmt in body {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let val = compile_expr(
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    value,
                    locals,
                    interp_alloca,
                )?;
                let alloca = builder
                    .build_alloca(*ptr_type, name)
                    .map_err(|e| format!("let alloca: {:?}", e))?;
                builder
                    .build_store(alloca, val)
                    .map_err(|e| format!("let store: {:?}", e))?;
                last_val = Some(val);
                locals.insert(name.clone(), alloca);
            }
            Stmt::Print { span: _, value, newline } => {
                let val = compile_expr(
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    value,
                    locals,
                    interp_alloca,
                )?;
                let print_fn = if *newline { rt.println } else { rt.print };
                builder
                    .build_call(print_fn, &[val.into()], "jit_print")
                    .map_err(|e| format!("print: {:?}", e))?;
                last_val = Some(val);
            }
            Stmt::Expr { span: _, expr } => {
                last_val = Some(compile_expr(
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    expr,
                    locals,
                    interp_alloca,
                )?);
            }
            Stmt::Trait { .. } => {}
            Stmt::Impl { .. } => {}
            Stmt::Macro { .. } => {}
            Stmt::Throw { .. } => {}
            Stmt::Yield { span: _, value } => {
                let val = compile_expr(builder, rt, ptr_type, f64_type, value, locals, interp_alloca)?;
                let interp = builder.build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                builder.build_call(rt.yield_fn, &[interp.into(), val.into()], "jit_yield")
                    .map_err(|e| format!("yield: {:?}", e))?;
                last_val = None;
            }
            Stmt::Return { span: _, value } => {
                let val = compile_expr(
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    value,
                    locals,
                    interp_alloca,
                )?;
                builder
                    .build_return(Some(&val as &dyn BasicValue))
                    .map_err(|e| format!("ret: {:?}", e))?;
                let dead = context.append_basic_block(function, "after_ret");
                builder.position_at_end(dead);
            }
            Stmt::If {
                span: _,
                condition,
                then_branch,
                else_branch,
            } => {
                compile_if(
                    context,
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    i8_type,
                    condition,
                    then_branch,
                    else_branch,
                    locals,
                    function,
                    loop_ctx,
                    interp_alloca,
                )?;
            }
            Stmt::While {
                span: _,
                condition,
                body: wbody,
            } => {
                compile_while(
                    context,
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    i8_type,
                    condition,
                    wbody,
                    locals,
                    function,
                    interp_alloca,
                )?;
            }
            Stmt::For {
                span: _,
                var,
                iterable,
                body: fbody,
            } => {
                compile_for(
                    context,
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    i8_type,
                    var,
                    iterable,
                    fbody,
                    locals,
                    function,
                    interp_alloca,
                )?;
            }
            Stmt::Break { .. } => {
                if let Some(lc) = loop_ctx {
                    builder
                        .build_unconditional_branch(lc.end_block)
                        .map_err(|e| format!("break: {:?}", e))?;
                    let dead = context.append_basic_block(function, "after_break");
                    builder.position_at_end(dead);
                }
            }
            Stmt::Continue { .. } => {
                if let Some(lc) = loop_ctx {
                    builder
                        .build_unconditional_branch(lc.cond_block)
                        .map_err(|e| format!("continue: {:?}", e))?;
                    let dead = context.append_basic_block(function, "after_cont");
                    builder.position_at_end(dead);
                }
            }
            Stmt::Match { value, .. } => {
                let val = compile_expr(
                    builder,
                    rt,
                    ptr_type,
                    f64_type,
                    value,
                    locals,
                    interp_alloca,
                )?;
                let i64_type = builder
                    .get_insert_block()
                    .map(|bb| bb.get_context())
                    .map(|ctx| ctx.i64_type())
                    .ok_or("no ctx")?;
                let stmt_addr = crate::jit_rt::jit_store_stmt((*stmt).clone());
                let addr_val = i64_type.const_int(stmt_addr as u64, false);
                let interp = builder
                    .build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                let call = builder
                    .build_call(
                        rt.exec_match,
                        &[interp.into(), val.into(), addr_val.into()],
                        "jit_match",
                    )
                    .map_err(|e| format!("match: {:?}", e))?;
                last_val = Some(ptr_from_call(call));
            }
            Stmt::Try { .. } => {
                let i64_type = builder
                    .get_insert_block()
                    .map(|bb| bb.get_context())
                    .map(|ctx| ctx.i64_type())
                    .ok_or("no ctx")?;
                let stmt_addr = crate::jit_rt::jit_store_stmt((*stmt).clone());
                let addr_val = i64_type.const_int(stmt_addr as u64, false);
                let interp = builder
                    .build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                let call = builder
                    .build_call(rt.exec_try, &[interp.into(), addr_val.into()], "jit_try")
                    .map_err(|e| format!("try: {:?}", e))?;
                last_val = Some(ptr_from_call(call));
            }
            Stmt::Import { .. } | Stmt::Struct { .. } | Stmt::Enum { .. } | Stmt::Fn { .. } | Stmt::Class { .. } | Stmt::DoWhile { .. } | Stmt::Destructure { .. } => {
                let i64_type = builder
                    .get_insert_block()
                    .map(|bb| bb.get_context())
                    .map(|ctx| ctx.i64_type())
                    .ok_or("no ctx")?;
                let stmt_addr = crate::jit_rt::jit_store_stmt((*stmt).clone());
                let addr_val = i64_type.const_int(stmt_addr as u64, false);
                let interp = builder
                    .build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                let call = builder
                    .build_call(rt.exec_nested_stmt, &[interp.into(), addr_val.into()], "jit_nested_stmt")
                    .map_err(|e| format!("nested stmt: {:?}", e))?;
                last_val = Some(ptr_from_call(call));
            }
        }
    }

    Ok(last_val)
}

fn compile_if<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    ptr_type: &inkwell::types::PointerType<'ctx>,
    f64_type: &inkwell::types::FloatType<'ctx>,
    i8_type: &IntType<'ctx>,
    condition: &Expr,
    then_branch: &[Stmt],
    else_branch: &Option<Vec<Stmt>>,
    locals: &mut HashMap<String, PointerValue<'ctx>>,
    function: FunctionValue<'ctx>,
    loop_ctx: Option<&LoopContext<'ctx>>,
    interp_alloca: PointerValue<'ctx>,
) -> Result<(), String> {
    let cond_val = compile_expr(
        builder,
        rt,
        ptr_type,
        f64_type,
        condition,
        locals,
        interp_alloca,
    )?;
    let cond_i1 = truthy_to_i1(builder, rt, i8_type, cond_val)?;

    let then_bb = context.append_basic_block(function, "if.then");
    let else_bb = context.append_basic_block(function, "if.else");
    let merge_bb = context.append_basic_block(function, "if.merge");

    builder
        .build_conditional_branch(cond_i1, then_bb, else_bb)
        .map_err(|e| format!("cond branch: {:?}", e))?;

    let then_ends_with_term = matches!(
        then_branch.last(),
        Some(Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. })
    );
    let else_ends_with_term = else_branch
        .as_ref()
        .map(|b| {
            matches!(
                b.last(),
                Some(Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. })
            )
        })
        .unwrap_or(false);

    builder.position_at_end(then_bb);
    compile_stmts(
        context,
        builder,
        rt,
        ptr_type,
        f64_type,
        i8_type,
        then_branch,
        locals,
        function,
        loop_ctx,
        interp_alloca,
    )?;
    if !then_ends_with_term {
        builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| format!("then -> merge: {:?}", e))?;
    }

    builder.position_at_end(else_bb);
    if let Some(eb) = else_branch {
        compile_stmts(
            context,
            builder,
            rt,
            ptr_type,
            f64_type,
            i8_type,
            eb,
            locals,
            function,
            loop_ctx,
            interp_alloca,
        )?;
    }
    if !else_ends_with_term {
        builder
            .build_unconditional_branch(merge_bb)
            .map_err(|e| format!("else -> merge: {:?}", e))?;
    }

    builder.position_at_end(merge_bb);
    Ok(())
}

fn compile_while<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    ptr_type: &inkwell::types::PointerType<'ctx>,
    f64_type: &inkwell::types::FloatType<'ctx>,
    i8_type: &IntType<'ctx>,
    condition: &Expr,
    body: &[Stmt],
    locals: &mut HashMap<String, PointerValue<'ctx>>,
    function: FunctionValue<'ctx>,
    interp_alloca: PointerValue<'ctx>,
) -> Result<(), String> {
    let cond_bb = context.append_basic_block(function, "while.cond");
    let body_bb = context.append_basic_block(function, "while.body");
    let end_bb = context.append_basic_block(function, "while.end");

    builder
        .build_unconditional_branch(cond_bb)
        .map_err(|e| format!("-> cond: {:?}", e))?;

    builder.position_at_end(cond_bb);
    let cond_val = compile_expr(
        builder,
        rt,
        ptr_type,
        f64_type,
        condition,
        locals,
        interp_alloca,
    )?;
    let cond_i1 = truthy_to_i1(builder, rt, i8_type, cond_val)?;
    builder
        .build_conditional_branch(cond_i1, body_bb, end_bb)
        .map_err(|e| format!("cond br: {:?}", e))?;

    let lc = LoopContext {
        cond_block: cond_bb,
        end_block: end_bb,
    };
    builder.position_at_end(body_bb);
    compile_stmts(
        context,
        builder,
        rt,
        ptr_type,
        f64_type,
        i8_type,
        body,
        locals,
        function,
        Some(&lc),
        interp_alloca,
    )?;
    if !matches!(
        body.last(),
        Some(Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. })
    ) {
        builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| format!("body -> cond: {:?}", e))?;
    }

    builder.position_at_end(end_bb);
    Ok(())
}

fn compile_for<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    ptr_type: &inkwell::types::PointerType<'ctx>,
    f64_type: &inkwell::types::FloatType<'ctx>,
    i8_type: &IntType<'ctx>,
    var: &str,
    iterable: &Expr,
    body: &[Stmt],
    locals: &mut HashMap<String, PointerValue<'ctx>>,
    function: FunctionValue<'ctx>,
    interp_alloca: PointerValue<'ctx>,
) -> Result<(), String> {
    let iter_val = compile_expr(
        builder,
        rt,
        ptr_type,
        f64_type,
        iterable,
        locals,
        interp_alloca,
    )?;
    let iter_ptr = builder
        .build_alloca(*ptr_type, "iter")
        .map_err(|e| format!("iter alloca: {:?}", e))?;
    builder
        .build_store(iter_ptr, iter_val)
        .map_err(|e| format!("store iter: {:?}", e))?;

    let iter_for_call = builder
        .build_load(*ptr_type, iter_ptr, "iter_load")
        .map_err(|e| format!("load iter: {:?}", e))?
        .into_pointer_value();
    let iter_made = builder
        .build_call(rt.make_iter, &[iter_for_call.into()], "make_iter")
        .map_err(|e| format!("make_iter: {:?}", e))?;
    builder
        .build_store(iter_ptr, ptr_from_call(iter_made))
        .map_err(|e| format!("store made_iter: {:?}", e))?;

    let var_alloca = builder
        .build_alloca(*ptr_type, var)
        .map_err(|e| format!("var alloca: {:?}", e))?;
    locals.insert(var.to_string(), var_alloca);

    let cond_bb = context.append_basic_block(function, "for.cond");
    let body_bb = context.append_basic_block(function, "for.body");
    let end_bb = context.append_basic_block(function, "for.end");

    builder
        .build_unconditional_branch(cond_bb)
        .map_err(|e| format!("-> cond: {:?}", e))?;

    builder.position_at_end(cond_bb);
    let iter_load = builder
        .build_load(*ptr_type, iter_ptr, "iter_for_next")
        .map_err(|e| format!("load iter2: {:?}", e))?
        .into_pointer_value();
    let next_val = builder
        .build_call(rt.iter_next, &[iter_load.into()], "next_val")
        .map_err(|e| format!("iter_next: {:?}", e))?;
    let next_ptr = ptr_from_call(next_val);

    let nil_i1 = is_nil_to_i1(builder, rt, i8_type, next_ptr)?;
    builder
        .build_conditional_branch(nil_i1, end_bb, body_bb)
        .map_err(|e| format!("for cond: {:?}", e))?;

    let lc = LoopContext {
        cond_block: cond_bb,
        end_block: end_bb,
    };
    builder.position_at_end(body_bb);
    builder
        .build_store(var_alloca, next_ptr)
        .map_err(|e| format!("store var: {:?}", e))?;
    compile_stmts(
        context,
        builder,
        rt,
        ptr_type,
        f64_type,
        i8_type,
        body,
        locals,
        function,
        Some(&lc),
        interp_alloca,
    )?;
    if !matches!(
        body.last(),
        Some(Stmt::Return { .. } | Stmt::Break { .. } | Stmt::Continue { .. })
    ) {
        builder
            .build_unconditional_branch(cond_bb)
            .map_err(|e| format!("body -> cond: {:?}", e))?;
    }

    builder.position_at_end(end_bb);
    Ok(())
}

fn compile_expr<'ctx>(
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    ptr_type: &inkwell::types::PointerType<'ctx>,
    f64_type: &inkwell::types::FloatType<'ctx>,
    expr: &Expr,
    locals: &HashMap<String, PointerValue<'ctx>>,
    interp_alloca: PointerValue<'ctx>,
) -> Result<PointerValue<'ctx>, String> {
    match expr {
        Expr::Int(n) => {
            let c = f64_type.const_float(*n as f64);
            let call = builder
                .build_call(rt.number, &[c.into()], "jit_number")
                .map_err(|e| format!("num: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Float(n) => {
            let c = f64_type.const_float(*n);
            let call = builder
                .build_call(rt.number, &[c.into()], "jit_number")
                .map_err(|e| format!("num: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Boolean(b) => {
            let ctx = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .ok_or("no block")?;
            let cb = ctx.bool_type().const_int(if *b { 1 } else { 0 }, false);
            let call = builder
                .build_call(rt.boolean, &[cb.into()], "jit_bool")
                .map_err(|e| format!("bool: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Nil => {
            let call = builder
                .build_call(rt.nil, &[], "jit_nil")
                .map_err(|e| format!("nil: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::String(s) => {
            let ctx = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .ok_or("no block")?;
            let gv = builder
                .build_global_string_ptr(s, "str")
                .map_err(|e| format!("global str: {:?}", e))?;
            let data_ptr = gv.as_pointer_value();
            let len = ctx.i64_type().const_int(s.len() as u64, false);
            let call = builder
                .build_call(
                    rt.string_from_data,
                    &[data_ptr.into(), len.into()],
                    "jit_str",
                )
                .map_err(|e| format!("str: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Variable(name) => {
            if let Some(alloca) = locals.get(name) {
                let loaded = builder
                    .build_load(*ptr_type, *alloca, &format!("load_{}", name))
                    .map_err(|e| format!("load var: {:?}", e))?
                    .into_pointer_value();
                Ok(loaded)
            } else {
                let ctx = builder
                    .get_insert_block()
                    .map(|bb| bb.get_context())
                    .ok_or("no ctx")?;
                let i64_type = ctx.i64_type();
                let gv = builder
                    .build_global_string_ptr(name, "global_name")
                    .map_err(|e| format!("global str: {:?}", e))?;
                let name_ptr = gv.as_pointer_value();
                let name_len = i64_type.const_int(name.len() as u64, false);
                let interp = builder
                    .build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                let call = builder
                    .build_call(
                        rt.get_global,
                        &[interp.into(), name_ptr.into(), name_len.into()],
                        "jit_get_global",
                    )
                    .map_err(|e| format!("get_global: {:?}", e))?;
                Ok(ptr_from_call(call))
            }
        }
        Expr::BinaryOp { left, op, right } => {
            let l = compile_expr(builder, rt, ptr_type, f64_type, left, locals, interp_alloca)?;
            let r = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                right,
                locals,
                interp_alloca,
            )?;
            let (f, n) = match op {
                BinaryOpKind::Add => (rt.add, "jit_add"),
                BinaryOpKind::Subtract => (rt.sub, "jit_sub"),
                BinaryOpKind::Multiply => (rt.mul, "jit_mul"),
                BinaryOpKind::Divide => (rt.div, "jit_div"),
                BinaryOpKind::Modulo => (rt.modulo, "jit_mod"),
                BinaryOpKind::Equal => (rt.eq, "jit_eq"),
                BinaryOpKind::NotEqual => (rt.neq, "jit_neq"),
                BinaryOpKind::Less => (rt.lt, "jit_lt"),
                BinaryOpKind::LessEqual => (rt.le, "jit_le"),
                BinaryOpKind::Greater => (rt.gt, "jit_gt"),
                BinaryOpKind::GreaterEqual => (rt.ge, "jit_ge"),
                BinaryOpKind::And => (rt.and, "jit_and"),
                BinaryOpKind::Or => (rt.or, "jit_or"),
                BinaryOpKind::In => (rt.in_op, "jit_in"),
                BinaryOpKind::BitAnd => (rt.bit_and, "jit_bit_and"),
                BinaryOpKind::BitOr => (rt.bit_or, "jit_bit_or"),
                BinaryOpKind::BitXor => (rt.bit_xor, "jit_bit_xor"),
                BinaryOpKind::ShiftLeft => (rt.shl, "jit_shl"),
                BinaryOpKind::ShiftRight => (rt.shr, "jit_shr"),
            };
            let call = builder
                .build_call(f, &[l.into(), r.into()], n)
                .map_err(|e| format!("binop: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::UnaryOp { op, right } => {
            let r = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                right,
                locals,
                interp_alloca,
            )?;
            let (f, n) = match op {
                UnaryOpKind::Negate => (rt.negate, "jit_negate"),
                UnaryOpKind::Not => (rt.not, "jit_not"),
                UnaryOpKind::BitNot => (rt.bit_not, "jit_bit_not"),
            };
            let call = builder
                .build_call(f, &[r.into()], n)
                .map_err(|e| format!("unop: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Assignment { name, value } => {
            let val = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                value,
                locals,
                interp_alloca,
            )?;
            if let Some(alloca) = locals.get(name) {
                builder
                    .build_store(*alloca, val)
                    .map_err(|e| format!("assign store: {:?}", e))?;
                Ok(val)
            } else {
                let ctx = builder
                    .get_insert_block()
                    .map(|bb| bb.get_context())
                    .ok_or("no ctx")?;
                let i64_type = ctx.i64_type();
                let gv = builder
                    .build_global_string_ptr(name, "global_name")
                    .map_err(|e| format!("global str: {:?}", e))?;
                let name_ptr = gv.as_pointer_value();
                let name_len = i64_type.const_int(name.len() as u64, false);
                let interp = builder
                    .build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                let call = builder
                    .build_call(
                        rt.set_global,
                        &[interp.into(), name_ptr.into(), name_len.into(), val.into()],
                        "jit_set_global",
                    )
                    .map_err(|e| format!("set_global: {:?}", e))?;
                Ok(ptr_from_call(call))
            }
        }
        Expr::CompoundAssign { name, op, value } => {
            let (f, n) = match op {
                BinaryOpKind::Add => (rt.add, "jit_add"),
                BinaryOpKind::Subtract => (rt.sub, "jit_sub"),
                BinaryOpKind::Multiply => (rt.mul, "jit_mul"),
                BinaryOpKind::Divide => (rt.div, "jit_div"),
                BinaryOpKind::Modulo => (rt.modulo, "jit_mod"),
                BinaryOpKind::Equal => (rt.eq, "jit_eq"),
                BinaryOpKind::NotEqual => (rt.neq, "jit_neq"),
                BinaryOpKind::Less => (rt.lt, "jit_lt"),
                BinaryOpKind::LessEqual => (rt.le, "jit_le"),
                BinaryOpKind::Greater => (rt.gt, "jit_gt"),
                BinaryOpKind::GreaterEqual => (rt.ge, "jit_ge"),
                BinaryOpKind::And => (rt.and, "jit_and"),
                BinaryOpKind::Or => (rt.or, "jit_or"),
                BinaryOpKind::In => (rt.in_op, "jit_in"),
                BinaryOpKind::BitAnd => (rt.bit_and, "jit_bit_and"),
                BinaryOpKind::BitOr => (rt.bit_or, "jit_bit_or"),
                BinaryOpKind::BitXor => (rt.bit_xor, "jit_bit_xor"),
                BinaryOpKind::ShiftLeft => (rt.shl, "jit_shl"),
                BinaryOpKind::ShiftRight => (rt.shr, "jit_shr"),
            };
            let rhs = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                value,
                locals,
                interp_alloca,
            )?;
            if let Some(alloca) = locals.get(name) {
                let current = builder
                    .build_load(*ptr_type, *alloca, &format!("load_{}", name))
                    .map_err(|e| format!("load: {:?}", e))?
                    .into_pointer_value();
                let call = builder
                    .build_call(f, &[current.into(), rhs.into()], n)
                    .map_err(|e| format!("compound: {:?}", e))?;
                let result = ptr_from_call(call);
                builder
                    .build_store(*alloca, result)
                    .map_err(|e| format!("store: {:?}", e))?;
                Ok(result)
            } else {
                // compound assign on global: read, apply op, write back
                let ctx = builder
                    .get_insert_block()
                    .map(|bb| bb.get_context())
                    .ok_or("no ctx")?;
                let i64_type = ctx.i64_type();
                let interp = builder
                    .build_load(*ptr_type, interp_alloca, "interp")
                    .map_err(|e| format!("load interp: {:?}", e))?
                    .into_pointer_value();
                let gv = builder
                    .build_global_string_ptr(name, "global_name")
                    .map_err(|e| format!("global str: {:?}", e))?;
                let name_ptr = gv.as_pointer_value();
                let name_len = i64_type.const_int(name.len() as u64, false);
                let current = builder
                    .build_call(
                        rt.get_global,
                        &[interp.into(), name_ptr.into(), name_len.into()],
                        "jit_get_global",
                    )
                    .map_err(|e| format!("get_global: {:?}", e))?;
                let current = ptr_from_call(current);
                let call = builder
                    .build_call(f, &[current.into(), rhs.into()], n)
                    .map_err(|e| format!("compound: {:?}", e))?;
                let result = ptr_from_call(call);
                builder
                    .build_call(
                        rt.set_global,
                        &[interp.into(), name_ptr.into(), name_len.into(), result.into()],
                        "jit_set_global",
                    )
                    .map_err(|e| format!("set_global: {:?}", e))?;
                Ok(result)
            }
        }
        Expr::Range { start, end } => {
            let sv = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                start,
                locals,
                interp_alloca,
            )?;
            let ev = compile_expr(builder, rt, ptr_type, f64_type, end, locals, interp_alloca)?;
            let call = builder
                .build_call(rt.range, &[sv.into(), ev.into()], "range")
                .map_err(|e| format!("range: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::List(items) => {
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let count = items.len();
            let args_array = builder
                .build_alloca(*ptr_type, "list_args")
                .map_err(|e| format!("list alloca: {:?}", e))?;
            for (i, item) in items.iter().enumerate() {
                let val =
                    compile_expr(builder, rt, ptr_type, f64_type, item, locals, interp_alloca)?;
                let idx = i64_type.const_int(i as u64, false);
                let gep = unsafe {
                    builder
                        .build_in_bounds_gep(
                            *ptr_type,
                            args_array,
                            &[idx],
                            &format!("list_item_{}", i),
                        )
                        .map_err(|e| format!("list gep: {:?}", e))?
                };
                builder
                    .build_store(gep, val)
                    .map_err(|e| format!("store item: {:?}", e))?;
            }
            let count_val = i64_type.const_int(count as u64, false);
            let call = builder
                .build_call(rt.list, &[args_array.into(), count_val.into()], "jit_list")
                .map_err(|e| format!("list: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Dict(pairs) => {
            let dict = builder
                .build_call(rt.dict, &[], "jit_dict")
                .map_err(|e| format!("dict: {:?}", e))?;
            let dict_ptr = ptr_from_call(dict);
            for (k, v) in pairs {
                let kv = compile_expr(builder, rt, ptr_type, f64_type, k, locals, interp_alloca)?;
                let vv = compile_expr(builder, rt, ptr_type, f64_type, v, locals, interp_alloca)?;
                builder
                    .build_call(
                        rt.dict_set,
                        &[dict_ptr.into(), kv.into(), vv.into()],
                        "dict_set",
                    )
                    .map_err(|e| format!("dict_set: {:?}", e))?;
            }
            Ok(dict_ptr)
        }
        Expr::Index { object, index } => {
            let obj = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                object,
                locals,
                interp_alloca,
            )?;
            let idx = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                index,
                locals,
                interp_alloca,
            )?;
            let call = builder
                .build_call(rt.index_get, &[obj.into(), idx.into()], "index_get")
                .map_err(|e| format!("index: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Call { callee, args } => {
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let count = args.len();
            let args_array = builder
                .build_alloca(*ptr_type, "call_args")
                .map_err(|e| format!("call alloca: {:?}", e))?;
            for (i, arg) in args.iter().enumerate() {
                let val =
                    compile_expr(builder, rt, ptr_type, f64_type, arg, locals, interp_alloca)?;
                let idx = i64_type.const_int(i as u64, false);
                let gep = unsafe {
                    builder
                        .build_in_bounds_gep(*ptr_type, args_array, &[idx], &format!("arg_{}", i))
                        .map_err(|e| format!("arg gep: {:?}", e))?
                };
                builder
                    .build_store(gep, val)
                    .map_err(|e| format!("store arg: {:?}", e))?;
            }
            let name_data = builder
                .build_global_string_ptr(callee, "callee")
                .map_err(|e| format!("callee str: {:?}", e))?;
            let name_ptr = name_data.as_pointer_value();
            let name_len = i64_type.const_int(callee.len() as u64, false);
            let count_val = i64_type.const_int(count as u64, false);
            let interp = builder
                .build_load(*ptr_type, interp_alloca, "interp")
                .map_err(|e| format!("load interp: {:?}", e))?
                .into_pointer_value();
            let call = builder
                .build_call(
                    rt.call,
                    &[
                        interp.into(),
                        name_ptr.into(),
                        name_len.into(),
                        args_array.into(),
                        count_val.into(),
                    ],
                    "jit_call",
                )
                .map_err(|e| format!("call: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let obj_val = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                object,
                locals,
                interp_alloca,
            )?;
            let count = args.len();
            let args_array = builder
                .build_alloca(*ptr_type, "method_args")
                .map_err(|e| format!("method alloca: {:?}", e))?;
            for (i, arg) in args.iter().enumerate() {
                let val =
                    compile_expr(builder, rt, ptr_type, f64_type, arg, locals, interp_alloca)?;
                let idx = i64_type.const_int(i as u64, false);
                let gep = unsafe {
                    builder
                        .build_in_bounds_gep(*ptr_type, args_array, &[idx], &format!("marg_{}", i))
                        .map_err(|e| format!("marg gep: {:?}", e))?
                };
                builder
                    .build_store(gep, val)
                    .map_err(|e| format!("store marg: {:?}", e))?;
            }
            let name_data = builder
                .build_global_string_ptr(method, "method")
                .map_err(|e| format!("method str: {:?}", e))?;
            let name_ptr = name_data.as_pointer_value();
            let name_len = i64_type.const_int(method.len() as u64, false);
            let count_val = i64_type.const_int(count as u64, false);
            let interp = builder
                .build_load(*ptr_type, interp_alloca, "interp")
                .map_err(|e| format!("load interp: {:?}", e))?
                .into_pointer_value();
            let call = builder
                .build_call(
                    rt.method_call,
                    &[
                        interp.into(),
                        obj_val.into(),
                        name_ptr.into(),
                        name_len.into(),
                        args_array.into(),
                        count_val.into(),
                    ],
                    "jit_method_call",
                )
                .map_err(|e| format!("method_call: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::StringInterp(parts) => {
            if parts.is_empty() {
                let call = builder
                    .build_call(rt.nil, &[], "interp_empty")
                    .map_err(|e| format!("nil: {:?}", e))?;
                return Ok(ptr_from_call(call));
            }
            let mut result = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                &parts[0],
                locals,
                interp_alloca,
            )?;
            for part in &parts[1..] {
                let next =
                    compile_expr(builder, rt, ptr_type, f64_type, part, locals, interp_alloca)?;
                let call = builder
                    .build_call(rt.add, &[result.into(), next.into()], "interp_add")
                    .map_err(|e| format!("interp: {:?}", e))?;
                result = ptr_from_call(call);
            }
            Ok(result)
        }
        Expr::FieldAccess { object, field } => {
            let obj = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                object,
                locals,
                interp_alloca,
            )?;
            let field_data = builder
                .build_global_string_ptr(field, "field")
                .map_err(|e| format!("field str: {:?}", e))?;
            let field_ptr = field_data.as_pointer_value();
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let field_len = i64_type.const_int(field.len() as u64, false);
            let interp = builder
                .build_load(*ptr_type, interp_alloca, "interp")
                .map_err(|e| format!("load interp: {:?}", e))?
                .into_pointer_value();
            let call = builder
                .build_call(
                    rt.field_get,
                    &[
                        interp.into(),
                        obj.into(),
                        field_ptr.into(),
                        field_len.into(),
                    ],
                    "jit_field_get",
                )
                .map_err(|e| format!("field_get: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::FieldAssign { object, field, value } => {
            let obj = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                object,
                locals,
                interp_alloca,
            )?;
            let val = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                value,
                locals,
                interp_alloca,
            )?;
            let field_data = builder
                .build_global_string_ptr(field, "field")
                .map_err(|e| format!("field str: {:?}", e))?;
            let field_ptr = field_data.as_pointer_value();
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let field_len = i64_type.const_int(field.len() as u64, false);
            let interp = builder
                .build_load(*ptr_type, interp_alloca, "interp")
                .map_err(|e| format!("load interp: {:?}", e))?
                .into_pointer_value();
            let call = builder
                .build_call(
                    rt.field_set,
                    &[
                        interp.into(),
                        obj.into(),
                        field_ptr.into(),
                        field_len.into(),
                        val.into(),
                    ],
                    "jit_field_set",
                )
                .map_err(|e| format!("field_set: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Fn { .. } => {
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let expr_addr = crate::jit_rt::jit_store_expr((*expr).clone());
            let addr_val = i64_type.const_int(expr_addr as u64, false);
            let interp = builder
                .build_load(*ptr_type, interp_alloca, "interp")
                .map_err(|e| format!("load interp: {:?}", e))?
                .into_pointer_value();
            let call = builder
                .build_call(
                    rt.make_closure,
                    &[interp.into(), addr_val.into()],
                    "jit_closure",
                )
                .map_err(|e| format!("closure: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Await { value } => {
            let val = compile_expr(builder, rt, ptr_type, f64_type, value, locals, interp_alloca)?;
            let call = builder
                .build_call(rt.await_fn, &[val.into()], "jit_await")
                .map_err(|e| format!("await: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Ternary { condition, then_expr, else_expr } => {
            let context = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .ok_or("no ctx")?;
            let i8_type = context.i8_type();
            let cond_val = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                condition,
                locals,
                interp_alloca,
            )?;
            let cond_i1 = truthy_to_i1(builder, rt, &i8_type, cond_val)?;
            let function = builder
                .get_insert_block()
                .map(|bb| {
                    bb.get_parent().unwrap()
                })
                .ok_or("no parent function")?;
            let then_bb = context.append_basic_block(function, "tern.then");
            let else_bb = context.append_basic_block(function, "tern.else");
            let merge_bb = context.append_basic_block(function, "tern.merge");

            builder
                .build_conditional_branch(cond_i1, then_bb, else_bb)
                .map_err(|e| format!("tern branch: {:?}", e))?;

            builder.position_at_end(then_bb);
            let then_val = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                then_expr,
                locals,
                interp_alloca,
            )?;
            builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| format!("then -> merge: {:?}", e))?;

            builder.position_at_end(else_bb);
            let else_val = compile_expr(
                builder,
                rt,
                ptr_type,
                f64_type,
                else_expr,
                locals,
                interp_alloca,
            )?;
            builder
                .build_unconditional_branch(merge_bb)
                .map_err(|e| format!("else -> merge: {:?}", e))?;

            builder.position_at_end(merge_bb);
            let phi = builder
                .build_phi(*ptr_type, "tern.result")
                .map_err(|e| format!("phi: {:?}", e))?;
            phi.add_incoming(&[
                (&then_val, then_bb),
                (&else_val, else_bb),
            ]);
            Ok(phi.as_basic_value().into_pointer_value())
        }
        Expr::Tuple(items) => {
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let count = items.len();
            let args_array = builder
                .build_alloca(*ptr_type, "tuple_args")
                .map_err(|e| format!("tuple alloca: {:?}", e))?;
            for (i, item) in items.iter().enumerate() {
                let val =
                    compile_expr(builder, rt, ptr_type, f64_type, item, locals, interp_alloca)?;
                let idx = i64_type.const_int(i as u64, false);
                let gep = unsafe {
                    builder
                        .build_in_bounds_gep(*ptr_type, args_array, &[idx], &format!("tuple_item_{}", i))
                        .map_err(|e| format!("tuple gep: {:?}", e))?
                };
                builder
                    .build_store(gep, val)
                    .map_err(|e| format!("store tuple item: {:?}", e))?;
            }
            let count_val = i64_type.const_int(count as u64, false);
            let call = builder
                .build_call(rt.tuple_fn, &[args_array.into(), count_val.into()], "jit_tuple")
                .map_err(|e| format!("tuple: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Set(items) => {
            let i64_type = builder
                .get_insert_block()
                .map(|bb| bb.get_context())
                .map(|ctx| ctx.i64_type())
                .ok_or("no ctx")?;
            let count = items.len();
            let args_array = builder
                .build_alloca(*ptr_type, "set_args")
                .map_err(|e| format!("set alloca: {:?}", e))?;
            for (i, item) in items.iter().enumerate() {
                let val =
                    compile_expr(builder, rt, ptr_type, f64_type, item, locals, interp_alloca)?;
                let idx = i64_type.const_int(i as u64, false);
                let gep = unsafe {
                    builder
                        .build_in_bounds_gep(*ptr_type, args_array, &[idx], &format!("set_item_{}", i))
                        .map_err(|e| format!("set gep: {:?}", e))?
                };
                builder
                    .build_store(gep, val)
                    .map_err(|e| format!("store set item: {:?}", e))?;
            }
            let count_val = i64_type.const_int(count as u64, false);
            let call = builder
                .build_call(rt.set_fn, &[args_array.into(), count_val.into()], "jit_set")
                .map_err(|e| format!("set: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Slice { object, start, end } => {
            let obj = compile_expr(builder, rt, ptr_type, f64_type, object, locals, interp_alloca)?;
            let start_val = if let Some(s) = start {
                compile_expr(builder, rt, ptr_type, f64_type, s, locals, interp_alloca)?
            } else {
                let nil_ptr = builder.build_call(rt.nil, &[], "nil_start")
                    .map_err(|e| format!("nil: {:?}", e))?;
                ptr_from_call(nil_ptr)
            };
            let end_val = if let Some(e) = end {
                compile_expr(builder, rt, ptr_type, f64_type, e, locals, interp_alloca)?
            } else {
                let nil_ptr = builder.build_call(rt.nil, &[], "nil_end")
                    .map_err(|e| format!("nil: {:?}", e))?;
                ptr_from_call(nil_ptr)
            };
            let call = builder
                .build_call(rt.slice_fn, &[obj.into(), start_val.into(), end_val.into()], "jit_slice")
                .map_err(|e| format!("slice: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Spread(inner) => {
            let val = compile_expr(builder, rt, ptr_type, f64_type, inner, locals, interp_alloca)?;
            let call = builder
                .build_call(rt.spread_fn, &[val.into()], "jit_spread")
                .map_err(|e| format!("spread: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Try { expr } => {
            // Compile the inner expression, then call __jit_try_propagate
            let val = compile_expr(builder, rt, ptr_type, f64_type, expr, locals, interp_alloca)?;
            let call = builder
                .build_call(rt.try_propagate, &[val.into()], "jit_try")
                .map_err(|e| format!("try: {:?}", e))?;
            Ok(ptr_from_call(call))
        }
        Expr::Super => {
            return Err("super not supported in JIT".into());
        }
        Expr::Grouping(inner) => compile_expr(
            builder,
            rt,
            ptr_type,
            f64_type,
            inner,
            locals,
            interp_alloca,
        ),
        Expr::ListComp { .. } | Expr::SetComp { .. } | Expr::DictComp { .. } => {
            return Err("comprehensions not supported in JIT".into());
        }
    }
}

fn ptr_from_call<'ctx>(call: inkwell::values::CallSiteValue<'ctx>) -> PointerValue<'ctx> {
    use inkwell::values::ValueKind;
    match call.try_as_basic_value() {
        ValueKind::Basic(b) => b.into_pointer_value(),
        _ => unreachable!(),
    }
}

fn is_nil_to_i1<'ctx>(
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    i8_type: &IntType<'ctx>,
    val: PointerValue<'ctx>,
) -> Result<inkwell::values::IntValue<'ctx>, String> {
    let call = builder
        .build_call(rt.is_nil, &[val.into()], "is_nil_byte")
        .map_err(|e| format!("is_nil: {:?}", e))?;
    let i8_val = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(BasicValueEnum::IntValue(v)) => v,
        _ => return Err("expected i8 from is_nil".into()),
    };
    let zero = i8_type.const_zero();
    builder
        .build_int_compare(inkwell::IntPredicate::NE, i8_val, zero, "nil_i1")
        .map_err(|e| format!("cmp: {:?}", e))
}

fn truthy_to_i1<'ctx>(
    builder: &Builder<'ctx>,
    rt: &RuntimeFns<'ctx>,
    i8_type: &IntType<'ctx>,
    val: PointerValue<'ctx>,
) -> Result<inkwell::values::IntValue<'ctx>, String> {
    let call = builder
        .build_call(rt.truthy, &[val.into()], "cond_byte")
        .map_err(|e| format!("truthy: {:?}", e))?;
    let i8_val = match call.try_as_basic_value() {
        inkwell::values::ValueKind::Basic(BasicValueEnum::IntValue(v)) => v,
        _ => return Err("expected i8 from truthy".into()),
    };
    let zero = i8_type.const_zero();
    builder
        .build_int_compare(inkwell::IntPredicate::NE, i8_val, zero, "i1_cmp")
        .map_err(|e| format!("cmp: {:?}", e))
}
