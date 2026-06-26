use std::collections::HashMap;
use crate::ast::{BinaryOpKind, DestructureItem, DestructureTarget, Expr, MatchArm, MatchPattern, Stmt, Type, UnaryOpKind};

pub struct TypeChecker {
    symbols: Vec<HashMap<String, Type>>,
    fn_returns: Vec<Option<Type>>,
    pub errors: Vec<Diag>,
    pub warnings: Vec<Diag>,
    reachable: Vec<bool>,
    structs: HashMap<String, Vec<(String, Option<Type>)>>,
    enums: HashMap<String, Vec<String>>,
    /// Generic bindings for the current call chain
    generic_bindings: Vec<HashMap<String, Type>>,
    /// Trait definitions: name -> (param_types per method, ret_types per method)
    traits: HashMap<String, (Vec<Vec<Type>>, Vec<Option<Type>>)>,
    current_line: usize,
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub msg: String,
    pub line: usize,
}

impl Diag {
    pub fn new(msg: String, line: usize) -> Self {
        Diag { msg, line }
    }
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut globals = HashMap::new();
        for name in &["len", "str", "int", "float", "bool", "type", "input",
            "abs", "sin", "cos", "sqrt", "say", "shout",
            "read", "write", "lines", "assert", "clock", "exit",
            "push", "pop", "sort", "reverse", "join", "split", "trim",
            "upper", "lower", "contains", "replace", "floor", "ceil",
            "round", "max", "min", "pow", "log", "exp", "json_encode",
            "json_decode", "json_validate", "map", "filter", "fold", "take", "collect", "iter",
            "Ok", "Err", "Some",
            // Standard library modules — treated as any-typed for now
            "math", "fs", "os", "datetime", "json", "str", "list", "net",
            "random", "encoding", "set", "regex", "process"] {
            globals.insert(name.to_string(), Type::Any);
        }
        TypeChecker {
            symbols: vec![globals],
            fn_returns: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            reachable: vec![true],
            structs: HashMap::new(),
            enums: HashMap::new(),
            generic_bindings: Vec::new(),
            traits: HashMap::new(),
            current_line: 0,
        }
    }

    pub fn check(&mut self, stmts: &[Stmt]) -> Result<(), Vec<Diag>> {
        self.check_stmts(stmts);
        if self.errors.is_empty() { Ok(()) } else { Err(self.errors.clone()) }
    }

    pub fn check_old(&mut self, stmts: &[Stmt]) -> Result<(), Vec<String>> {
        self.check_stmts(stmts);
        if self.errors.is_empty() { Ok(()) } else {
            Err(self.errors.iter().map(|d| d.msg.clone()).collect())
        }
    }

    fn is_reachable(&self) -> bool {
        *self.reachable.last().unwrap_or(&true)
    }

    fn get_type(&self, name: &str) -> Option<Type> {
        for s in self.symbols.iter().rev() {
            if let Some(t) = s.get(name) { return Some(t.clone()); }
        }
        None
    }

    fn define(&mut self, name: &str, t: Type) {
        if let Some(scope) = self.symbols.last_mut() {
            scope.insert(name.into(), t);
        }
    }

    fn error(&mut self, msg: String) {
        self.errors.push(Diag::new(msg, self.current_line));
    }

    fn warn(&mut self, msg: String) {
        self.warnings.push(Diag::new(msg, self.current_line));
    }

    /// Resolve a type, substituting generics with their bindings
    fn resolve_type(&self, t: &Type) -> Type {
        match t {
            Type::Generic(name) => {
                for bindings in self.generic_bindings.iter().rev() {
                    if let Some(concrete) = bindings.get(name) {
                        return self.resolve_type(concrete);
                    }
                }
                t.clone()
            }
            Type::List(inner) => Type::List(Box::new(self.resolve_type(inner))),
            Type::Dict(k, v) => Type::Dict(Box::new(self.resolve_type(k)), Box::new(self.resolve_type(v))),
            Type::Fn(params, ret) => Type::Fn(
                params.iter().map(|p| self.resolve_type(p)).collect(),
                Box::new(self.resolve_type(ret)),
            ),
            Type::Result(ok, err) => Type::Result(
                Box::new(self.resolve_type(ok)),
                Box::new(self.resolve_type(err)),
            ),
            Type::Option(inner) => Type::Option(Box::new(self.resolve_type(inner))),
            Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| self.resolve_type(t)).collect()),
            Type::SelfType => {
                // Look up Self binding in symbol scope
                self.get_type("Self").unwrap_or(t.clone())
            }
            Type::TraitObject(_) => t.clone(),
            _ => t.clone(),
        }
    }

    /// Resolve `Self` type to a concrete type in type trees
    fn resolve_self_type(&self, t: &Type, concrete: &Type) -> Type {
        match t {
            Type::SelfType => concrete.clone(),
            Type::List(inner) => Type::List(Box::new(self.resolve_self_type(inner, concrete))),
            Type::Dict(k, v) => Type::Dict(
                Box::new(self.resolve_self_type(k, concrete)),
                Box::new(self.resolve_self_type(v, concrete)),
            ),
            Type::Fn(params, ret) => Type::Fn(
                params.iter().map(|p| self.resolve_self_type(p, concrete)).collect(),
                Box::new(self.resolve_self_type(ret, concrete)),
            ),
            Type::Result(ok, err) => Type::Result(
                Box::new(self.resolve_self_type(ok, concrete)),
                Box::new(self.resolve_self_type(err, concrete)),
            ),
            Type::Option(inner) => Type::Option(Box::new(self.resolve_self_type(inner, concrete))),
            Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| self.resolve_self_type(t, concrete)).collect()),
            _ => t.clone(),
        }
    }

    /// Define `Self` → concrete type in the current scope
    fn define_self_type(&mut self, concrete: &Type) {
        self.define("Self", concrete.clone());
    }

    fn check_enum_exhaustiveness(&mut self, enum_name: &str, variants: &[String], arms: &[MatchArm]) {
        // Check if any arm uses a wildcard or a non-variant binding (catches everything)
        let has_catch_all = arms.iter().any(|arm| {
            match &arm.pattern {
                MatchPattern::Wildcard => true,
                MatchPattern::Binding(name) => !variants.contains(name),
                _ => false,
            }
        });
        if has_catch_all {
            return;
        }
        let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();
        for arm in arms {
            self.collect_covered_enum_variants(&arm.pattern, variants, &mut covered);
        }
        for variant in variants {
            if !covered.contains(variant) {
                self.error(format!(
                    "Match on enum '{}' does not cover variant '{}'",
                    enum_name, variant
                ));
            }
        }
    }

    fn collect_covered_enum_variants(&self, pattern: &MatchPattern, variants: &[String], covered: &mut std::collections::HashSet<String>) {
        match pattern {
            MatchPattern::Wildcard | MatchPattern::Binding(_) => {
                // Binding names that match variant names are treated as variant refs
                // but are already handled in has_catch_all above; for non-catch-all bindings
                // (i.e. binding names that match variants), we need to handle them here.
                if let MatchPattern::Binding(name) = pattern {
                    if variants.contains(name) {
                        covered.insert(name.clone());
                    }
                }
            }
            MatchPattern::Destructure(name, _) => {
                covered.insert(name.clone());
            }
            MatchPattern::Or(ps) => {
                for p in ps {
                    self.collect_covered_enum_variants(p, variants, covered);
                }
            }
            MatchPattern::Literal(_) => {}
        }
    }

    fn check_stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            self.check_stmt(s);
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        if !self.is_reachable() {
            self.warn("Dead code after return/break/continue".into());
        }
        // Track current line from statement span
        if let Some(span) = stmt.span() {
            self.current_line = span.line;
        }
        match stmt {
            Stmt::Let { span: _, pub_flag: _, name, type_ann, value } => {
                let vt = self.infer_expr(value);
                let expected = type_ann.as_ref().map(|t| self.resolve_type(t));
                if let Some(ref expected) = expected && !self.types_compatible(&vt, expected) {
                    self.error(format!("Type error: '{}' expected '{}', got '{}'", name, expected, vt));
                }
                self.define(name, vt);
            }
            Stmt::Struct { span: _, pub_flag: _, name, fields } => {
                self.structs.insert(name.clone(), fields.clone());
                self.define(name, Type::Instance(name.clone()));
            }
            Stmt::Enum { span: _, pub_flag: _, name, variants } => {
                let vnames: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                self.enums.insert(name.clone(), vnames.clone());
                self.define(name, Type::Instance(name.clone()));
            }
            Stmt::Class { span: _, pub_flag: _, name, extends: _, methods } => {
                for m in methods {
                    self.symbols.push(HashMap::new());
                    self.reachable.push(true);
                    for (pn, pt, _pd) in &m.params {
                        self.define(pn, pt.clone().unwrap_or(Type::Any));
                    }
                    self.fn_returns.push(None);
                    self.check_stmts(&m.body);
                    self.fn_returns.pop();
                    self.reachable.pop();
                    self.symbols.pop();
                }
                self.define(name, Type::Instance(name.clone()));
            }
            Stmt::Fn { span: _, pub_flag: _, name, generic_params, params, return_type, body, is_async: _ } => {
                self.symbols.push(HashMap::new());
                self.reachable.push(true);
                for gp in generic_params {
                    self.define(gp, Type::Generic(gp.clone()));
                }
                for (pn, pt, _pd) in params {
                    self.define(pn, pt.clone().unwrap_or(Type::Any));
                }
                self.fn_returns.push(return_type.clone());
                for s in body {
                    self.check_stmt(s);
                }
                // Infer return type from last expression if no explicit return type
                let inferred_ret = if return_type.is_none() || *return_type.as_ref().unwrap() == Type::Any {
                    if let Some(Stmt::Expr { span: _, expr }) = body.last() {
                        let t = self.infer_expr(expr);
                        if t != Type::Nil { Some(t) } else { None }
                    } else {
                        None
                    }
                } else {
                    None
                };
                // Check if function with return type actually has a return
                if let Some(rt) = return_type {
                    let rt = self.resolve_type(rt);
                    let had_return = body.iter().any(|s| matches!(s, Stmt::Return { .. }));
                    let last_is_expr = matches!(body.last(), Some(Stmt::Expr { .. }));
                    if !had_return && !last_is_expr && rt != Type::Any && rt != Type::Nil {
                        self.error(format!("Function '{}' has return type '{}' but no return statement", name, rt));
                    }
                }
                self.fn_returns.pop();
                self.reachable.pop();
                self.symbols.pop();
                let ret = inferred_ret.or_else(|| return_type.clone()).unwrap_or(Type::Any);
                self.define(name, Type::Fn(
                    params.iter().map(|(_, t, _)| t.clone().unwrap_or(Type::Any)).collect(),
                    Box::new(ret),
                ));
            }
            Stmt::If { span: _, condition: _, then_branch, else_branch } => {
                self.check_stmts(then_branch);
                let then_reachable = self.is_reachable();
                if let Some(eb) = else_branch {
                    self.check_stmts(eb);
                    let else_reachable = self.is_reachable();
                    if !then_reachable && !else_reachable {
                        *self.reachable.last_mut().unwrap() = false;
                    }
                }
            }
            Stmt::While { span: _, condition: _, body } => {
                self.check_stmts(body);
            }
            Stmt::DoWhile { span: _, condition: _, body } => {
                self.check_stmts(body);
            }
            Stmt::Destructure { span: _, pub_flag: _, target, value } => {
                let vt = self.infer_expr(value);
                match target {
                    DestructureTarget::List(items) => {
                        let elem_type = match &vt {
                            Type::List(inner) => *inner.clone(),
                            _ => Type::Any,
                        };
                        for item in items {
                            match item {
                                DestructureItem::Name(name) => self.define(name, elem_type.clone()),
                                DestructureItem::Rest(name) => self.define(name, Type::List(Box::new(elem_type.clone()))),
                            }
                        }
                    }
                    DestructureTarget::Struct(fields) => {
                        for field in fields {
                            self.define(field, Type::Any);
                        }
                    }
                }
            }
            Stmt::For { span: _, var, iterable, body } => {
                let it = self.infer_expr(iterable);
                let elem_type = match &it {
                    Type::List(inner) => *inner.clone(),
                    Type::String => Type::String,
                    _ => Type::Any,
                };
                self.symbols.push(HashMap::new());
                self.reachable.push(true);
                self.define(var, elem_type);
                self.check_stmts(body);
                self.reachable.pop();
                self.symbols.pop();
            }
            Stmt::Match { span: _, value, arms } => {
                let matched_type = self.infer_expr(value);
                // Check exhaustiveness for enum types
                if let Type::Instance(name) = &matched_type {
                    let variants = self.enums.get(name).cloned();
                    if let Some(variants) = variants {
                        self.check_enum_exhaustiveness(name, &variants, arms);
                    }
                }
                let enum_variants = if let Type::Instance(name) = &matched_type {
                    self.enums.get(name).cloned()
                } else {
                    None
                };
                let mut any_reachable = false;
                for arm in arms {
                    self.symbols.push(HashMap::new());
                    self.reachable.push(true);
                    self.bind_pattern(&arm.pattern, enum_variants.as_deref());
                    if let Some(guard) = &arm.guard {
                        self.infer_expr(guard);
                    }
                    self.check_stmts(&arm.body);
                    let reachable = self.is_reachable();
                    self.reachable.pop();
                    self.symbols.pop();
                    if reachable {
                        any_reachable = true;
                    }
                }
                if !any_reachable {
                    *self.reachable.last_mut().unwrap() = false;
                }
            }
            Stmt::Try { span: _, body, catch_var: _, catch_body } => {
                self.check_stmts(body);
                let try_reachable = self.is_reachable();
                self.check_stmts(catch_body);
                let catch_reachable = self.is_reachable();
                if !try_reachable && !catch_reachable {
                    *self.reachable.last_mut().unwrap() = false;
                }
            }
            Stmt::Return { span: _, value } => {
                let vt = self.infer_expr(value);
                if let Some(expected) = self.fn_returns.last().and_then(|r| r.clone())
                    && !self.types_compatible(&vt, &expected) {
                    self.error(format!("Type error: return expected '{}', got '{}'", expected, vt));
                }
                *self.reachable.last_mut().unwrap() = false;
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {
                *self.reachable.last_mut().unwrap() = false;
            }
            Stmt::Print { span: _, value, newline: _ } => { self.infer_expr(value); }
            Stmt::Import { span: _, pub_flag: _, path: _, alias } => {
                if let Some(a) = alias {
                    self.define(a, Type::Any);
                }
            }
            Stmt::Expr { span: _, expr } => { self.infer_expr(expr); }
            Stmt::Yield { span: _, value } => { self.infer_expr(value); }
            Stmt::Throw { span: _, value } => { self.infer_expr(value); }
            Stmt::Trait { span: _, pub_flag: _, name, methods } => {
                // Store trait definition: define the trait name
                let param_types: Vec<Vec<Type>> = methods.iter().map(|m| {
                    m.params.iter().map(|(_, t, _)| t.clone().unwrap_or(Type::Any)).collect()
                }).collect();
                let ret_types: Vec<Option<Type>> = methods.iter().map(|m| m.return_type.clone()).collect();
                self.traits.insert(name.clone(), (param_types, ret_types));
                // Type-check default method bodies
                for m in methods {
                    if let Some(body) = &m.body {
                        self.symbols.push(HashMap::new());
                        self.reachable.push(true);
                        for (pn, pt, _pd) in &m.params {
                            self.define(pn, pt.clone().unwrap_or(Type::Any));
                        }
                        self.fn_returns.push(m.return_type.clone());
                        for s in body {
                            self.check_stmt(s);
                        }
                        self.fn_returns.pop();
                        self.reachable.pop();
                        self.symbols.pop();
                    }
                }
                self.define(name, Type::TraitObject(name.clone()));
            }
            Stmt::Macro { .. } => {}
            Stmt::Impl { span: _, trait_name, type_name, methods } => {
                // Verify impl matches trait
                let trait_info = self.traits.get(trait_name).cloned();
                let concrete = Type::Instance(type_name.clone());
                if let Some((trait_params, trait_rets)) = trait_info {
                    // Check each method signature and type-check body
                    for (i, m) in methods.iter().enumerate() {
                        if i >= trait_params.len() {
                            self.error(format!(
                                "Impl for trait '{}' has more methods than expected",
                                trait_name
                            ));
                            break;
                        }
                        // Check param count
                        if m.params.len() != trait_params[i].len() {
                            self.error(format!(
                                "Method '{}' in impl of '{}' expects {} params, trait has {}",
                                m.name, trait_name, m.params.len(), trait_params[i].len()
                            ));
                        }
                        // Check return type (with Self substitution)
                        if let Some(trait_ret) = &trait_rets[i] {
                            let resolved_trait_ret = self.resolve_self_type(trait_ret, &concrete);
                            if let Some(impl_ret) = &m.return_type {
                                if !self.types_compatible(impl_ret, &resolved_trait_ret) {
                                    self.error(format!(
                                        "Method '{}' return type '{}' doesn't match trait '{}'",
                                        m.name, impl_ret, resolved_trait_ret
                                    ));
                                }
                            }
                        }
                        // Type-check method body with Self → concrete type
                        self.symbols.push(HashMap::new());
                        self.reachable.push(true);
                        // Define Self type alias in the method scope
                        self.define_self_type(&concrete);
                        for (pn, pt, _pd) in &m.params {
                            let resolved_pt = pt.as_ref().map(|t| self.resolve_self_type(t, &concrete));
                            self.define(pn, resolved_pt.unwrap_or(Type::Any));
                        }
                        let resolved_ret = m.return_type.as_ref().map(|t| self.resolve_self_type(t, &concrete));
                        self.fn_returns.push(resolved_ret);
                        for s in &m.body {
                            self.check_stmt(s);
                        }
                        self.fn_returns.pop();
                        self.reachable.pop();
                        self.symbols.pop();
                    }
                } else {
                    self.error(format!("Trait '{}' not defined", trait_name));
                }
            }
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> Type {
        match expr {
            Expr::Int(_) => Type::Int,
            Expr::Float(_) => Type::Float,
            Expr::String(_) => Type::String,
            Expr::Boolean(_) => Type::Bool,
            Expr::Nil => Type::Nil,
            Expr::Variable(name) => {
                self.get_type(name).unwrap_or_else(|| {
                    self.error(format!("Type error: undefined '{}'", name));
                    Type::Any
                })
            }
            Expr::Fn { generic_params, params, body } => {
                self.symbols.push(HashMap::new());
                self.reachable.push(true);
                for gp in generic_params {
                    self.define(gp, Type::Generic(gp.clone()));
                }
                for (pn, pt, _pd) in params {
                    self.define(pn, pt.clone().unwrap_or(Type::Any));
                }
                self.check_stmts(body);
                self.reachable.pop();
                self.symbols.pop();
                Type::Fn(
                    params.iter().map(|(_, t, _)| t.clone().unwrap_or(Type::Any)).collect(),
                    Box::new(Type::Any),
                )
            }
            Expr::List(items) => {
                if items.is_empty() { return Type::List(Box::new(Type::Any)); }
                let elem = self.infer_expr(&items[0]);
                Type::List(Box::new(elem))
            }
            Expr::Set(items) => {
                if items.is_empty() { return Type::List(Box::new(Type::Any)); }
                Type::List(Box::new(Type::Any))
            }
            Expr::Dict(pairs) => {
                if pairs.is_empty() { return Type::Dict(Box::new(Type::Any), Box::new(Type::Any)); }
                let kt = self.infer_expr(&pairs[0].0);
                let vt = self.infer_expr(&pairs[0].1);
                Type::Dict(Box::new(kt), Box::new(vt))
            }
            Expr::Index { object, index: _ } => {
                match self.infer_expr(object) {
                    Type::List(inner) => *inner,
                    Type::String => Type::String,
                    Type::Dict(_, vt) => *vt,
                    _ => Type::Any,
                }
            }
            Expr::Range { start: _, end: _ } => Type::Range,
            Expr::Tuple(items) => {
                if items.is_empty() { return Type::Tuple(Vec::new()); }
                let ts: Vec<Type> = items.iter().map(|i| self.infer_expr(i)).collect();
                Type::Tuple(ts)
            }
            Expr::Slice { object, .. } => self.infer_expr(object),
            Expr::Spread(inner) => self.infer_expr(inner),
            Expr::UnaryOp { op: _, right } => {
                let _ = self.infer_expr(right);
                match expr {
                    Expr::UnaryOp { op: UnaryOpKind::Negate, .. } => Type::Float,
                    Expr::UnaryOp { op: UnaryOpKind::Not, .. } => Type::Bool,
                    _ => Type::Any,
                }
            }
            Expr::BinaryOp { left, op, right } => {
                let _lt = self.infer_expr(left);
                let _rt = self.infer_expr(right);
                match op {
                    BinaryOpKind::Add | BinaryOpKind::Subtract
                    | BinaryOpKind::Multiply | BinaryOpKind::Divide
                    | BinaryOpKind::Modulo => Type::Float,
                    BinaryOpKind::Equal | BinaryOpKind::NotEqual
                    | BinaryOpKind::Less | BinaryOpKind::LessEqual
                    | BinaryOpKind::Greater | BinaryOpKind::GreaterEqual
                    | BinaryOpKind::And | BinaryOpKind::Or | BinaryOpKind::In => Type::Bool,
                    BinaryOpKind::BitAnd | BinaryOpKind::BitOr | BinaryOpKind::BitXor
                    | BinaryOpKind::ShiftLeft | BinaryOpKind::ShiftRight => Type::Int,
                }
            }
            Expr::Call { callee, args } => {
                let arg_types: Vec<Type> = args.iter().map(|a| self.infer_expr(a)).collect();
                // Builtin constructors for Result/Option
                match callee.as_str() {
                    "Ok" => {
                        if arg_types.len() != 1 {
                            self.error(format!("Ok() expects 1 arg, got {}", arg_types.len()));
                            return Type::Any;
                        }
                        return Type::Result(Box::new(arg_types[0].clone()), Box::new(Type::Any));
                    }
                    "Err" => {
                        if arg_types.len() != 1 {
                            self.error(format!("Err() expects 1 arg, got {}", arg_types.len()));
                            return Type::Any;
                        }
                        return Type::Result(Box::new(Type::Any), Box::new(arg_types[0].clone()));
                    }
                    "Some" => {
                        if arg_types.len() != 1 {
                            self.error(format!("Some() expects 1 arg, got {}", arg_types.len()));
                            return Type::Any;
                        }
                        return Type::Option(Box::new(arg_types[0].clone()));
                    }
                    _ => {}
                }
                let func_type = self.get_type(callee);
                match func_type {
                    Some(Type::Fn(param_types, ret_type)) => {
                        // Check argument count
                        if arg_types.len() != param_types.len() {
                            self.error(format!(
                                "Type error: '{}' expects {} args, got {}",
                                callee, param_types.len(), arg_types.len()
                            ));
                            return Type::Any;
                        }
                        // Bind generics based on argument types
                        let mut bindings = HashMap::new();
                        for (param_t, arg_t) in param_types.iter().zip(&arg_types) {
                            self.unify_types(param_t, arg_t, &mut bindings);
                        }
                        self.generic_bindings.push(bindings);
                        let resolved_ret = self.resolve_type(&ret_type);
                        self.generic_bindings.pop();
                        // Check argument compatibility
                        for (i, (param_t, arg_t)) in param_types.iter().zip(&arg_types).enumerate() {
                            let resolved_param = self.resolve_type(param_t);
                            if !self.types_compatible(arg_t, &resolved_param) {
                                self.error(format!(
                                    "Type error: '{}' param {} expected '{}', got '{}'",
                                    callee, i + 1, resolved_param, arg_t
                                ));
                            }
                        }
                        resolved_ret
                    }
                    Some(Type::Instance(_)) => {
                        // Struct constructor call
                        let name = callee.clone();
                        let fields_opt = self.structs.get(&name).cloned();
                        if let Some(fields) = &fields_opt {
                            if arg_types.len() != fields.len() {
                                self.error(format!(
                                    "Type error: struct '{}' expects {} fields, got {}",
                                    name, fields.len(), arg_types.len()
                                ));
                            } else {
                                for ((fname, ft), arg_t) in fields.iter().zip(&arg_types) {
                                    if let Some(ft) = ft {
                                        if !self.types_compatible(arg_t, ft) {
                                            self.error(format!(
                                                "Type error: struct '{}' field '{}' expected '{}', got '{}'",
                                                name, fname, ft, arg_t
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        Type::Instance(name)
                    }
                    Some(t) => t,
                    None => {
                        self.error(format!("Type error: undefined function '{}'", callee));
                        Type::Any
                    }
                }
            }
            Expr::MethodCall { object, method, args } => {
                let obj_type = self.infer_expr(object);
                for a in args { self.infer_expr(a); }
                // Check if this is an enum variant constructor call (e.g., Option.Some(42))
                if let Type::Instance(name) = &obj_type {
                    if let Some(variants) = self.enums.get(name) {
                        if variants.iter().any(|v| v == method) {
                            return obj_type;
                        }
                    }
                }
                Type::Any
            }
            Expr::Assignment { name, value } => {
                let vt = self.infer_expr(value);
                if let Some(expected) = self.get_type(name) && !self.types_compatible(&vt, &expected) {
                    self.error(format!("Type error: assigning '{}' to '{}', expected '{}'", name, vt, expected));
                }
                vt
            }
            Expr::CompoundAssign { name, op: _, value } => {
                self.infer_expr(value);
                self.get_type(name).unwrap_or(Type::Any)
            }
            Expr::StringInterp(parts) => {
                for p in parts { self.infer_expr(p); }
                Type::String
            }
            Expr::Grouping(inner) => self.infer_expr(inner),
            Expr::Await { value } => self.infer_expr(value),
            Expr::FieldAccess { object, field } => {
                let obj_type = self.infer_expr(object);
                match &obj_type {
                    Type::Instance(name) => {
                        if let Some(fields) = self.structs.get(name) {
                            for (fname, ft) in fields {
                                if fname == field {
                                    return ft.clone().unwrap_or(Type::Any);
                                }
                            }
                            self.error(format!("Field '{}' not found in struct '{}'", field, name));
                            Type::Any
                        } else if let Some(_variants) = self.enums.get(name) {
                            // Enum variant access: Color.Red has type Color
                            obj_type
                        } else {
                            self.error(format!("Type '{}' is not a struct or enum", name));
                            Type::Any
                        }
                    }
                    _ => {
                        self.error(format!("Cannot access field '{}' on '{}'", field, obj_type));
                        Type::Any
                    }
                }
            }
            Expr::FieldAssign { object, field, value } => {
                let obj_type = self.infer_expr(object);
                let val_type = self.infer_expr(value);
                match &obj_type {
                    Type::Instance(name) => {
                        if let Some(fields) = self.structs.get(name) {
                            let found = fields.iter().any(|(fname, _)| fname == field);
                            if !found {
                                self.error(format!("Field '{}' not found in '{}'", field, name));
                            }
                        } else if self.enums.contains_key(name) {
                            self.error(format!("Cannot assign to enum variant '{}'", name));
                        } else {
                            self.error(format!("Type '{}' is not a struct or enum", name));
                        }
                    }
                    _ => {
                        self.error(format!("Cannot assign to field '{}' on '{}'", field, obj_type));
                    }
                }
                val_type
            }
            Expr::Ternary { condition, then_expr, else_expr } => {
                self.infer_expr(condition);
                let t = self.infer_expr(then_expr);
                let e = self.infer_expr(else_expr);
                if t != e {
                    self.error(format!("Ternary arms have different types: '{}' and '{}'", t, e));
                }
                t
            }
            Expr::Super => Type::Any,
            Expr::Try { expr } => {
                let inner = self.infer_expr(expr);
                // Validate that current function returns Result or Option
                if let Some(Some(ret_type)) = self.fn_returns.last() {
                    match ret_type {
                        Type::Result(ok_t, _) => {
                            // Also verify unwrapped type matches Ok variant
                            if let Type::Result(ref actual_ok, _) = inner {
                                if !self.types_compatible(actual_ok, ok_t) {
                                    self.error(format!(
                                        "Type error: '?' unwraps '{}', but function returns Result<{}, _>",
                                        actual_ok, ok_t
                                    ));
                                }
                            }
                        }
                        Type::Option(inner_t) => {
                            if let Type::Option(ref actual_inner) = inner {
                                if !self.types_compatible(actual_inner, inner_t) {
                                    self.error(format!(
                                        "Type error: '?' unwraps '{}', but function returns Option<{}>",
                                        actual_inner, inner_t
                                    ));
                                }
                            }
                        }
                        _ => {
                            self.error(format!(
                                "'?' used in function returning '{}', but Result or Option required",
                                ret_type
                            ));
                        }
                    }
                }
                match inner {
                    Type::Result(ok, _) => *ok,
                    Type::Option(inner) => *inner,
                    _ => {
                        self.error("Try operator '?' requires Result or Option type".into());
                        inner
                    }
                }
            }
            Expr::ListComp { expr, clauses } => {
                // Infer element type from the output expression
                let elem = self.infer_expr(expr);
                for c in clauses {
                    self.define(&c.var, Type::Any);
                }
                Type::List(Box::new(elem))
            }
            Expr::SetComp { expr, clauses } => {
                let elem = self.infer_expr(expr);
                for c in clauses {
                    self.define(&c.var, Type::Any);
                }
                Type::List(Box::new(elem))
            }
            Expr::DictComp { key, value, clauses } => {
                let k = self.infer_expr(key);
                let v = self.infer_expr(value);
                for c in clauses {
                    self.define(&c.var, Type::Any);
                }
                Type::Dict(Box::new(k), Box::new(v))
            }
        }
    }

    fn bind_pattern(&mut self, pattern: &crate::ast::MatchPattern, enum_variants: Option<&[String]>) {
        match pattern {
            crate::ast::MatchPattern::Binding(name) => {
                // Don't bind names that are enum variants (they're variant references, not bindings)
                if let Some(variants) = enum_variants {
                    if variants.contains(name) {
                        return;
                    }
                }
                self.define(name, Type::Any);
            }
            crate::ast::MatchPattern::Destructure(name, fields) => {
                let field_types: Vec<(String, Type)> = if let Some(struct_fields) = self.structs.get(name) {
                    fields.iter().map(|fname| {
                        let ft = struct_fields.iter()
                            .find(|(sfn, _)| sfn == fname)
                            .and_then(|(_, t)| t.clone())
                            .unwrap_or(Type::Any);
                        (fname.clone(), ft)
                    }).collect()
                } else {
                    fields.iter().map(|fname| (fname.clone(), Type::Any)).collect()
                };
                for (fname, ft) in field_types {
                    if fname != "_" {
                        self.define(&fname, ft);
                    }
                }
            }
            crate::ast::MatchPattern::Or(patterns) => {
                for p in patterns {
                    self.bind_pattern(p, enum_variants);
                }
            }
            _ => {}
        }
    }

    /// Unify two types, binding generics in `bindings`
    fn unify_types(&self, param: &Type, actual: &Type, bindings: &mut HashMap<String, Type>) {
        match (param, actual) {
            (Type::Generic(name), _) => {
                if !bindings.contains_key(name) {
                    bindings.insert(name.clone(), actual.clone());
                }
            }
            (Type::List(p), Type::List(a)) => self.unify_types(p, a, bindings),
            (Type::Dict(pk, pv), Type::Dict(ak, av)) => {
                self.unify_types(pk, ak, bindings);
                self.unify_types(pv, av, bindings);
            }
            (Type::Fn(pp, pr), Type::Fn(ap, ar)) => {
                for (p, a) in pp.iter().zip(ap) {
                    self.unify_types(p, a, bindings);
                }
                self.unify_types(pr, ar, bindings);
            }
            _ => {}
        }
    }

    fn types_compatible(&self, actual: &Type, expected: &Type) -> bool {
        let expected = self.resolve_type(expected);
        let actual = self.resolve_type(actual);
        if expected == Type::Any { return true; }
        match (&actual, &expected) {
            (Type::Int, Type::Int) | (Type::Int, Type::Float) => true,
            (Type::Float, Type::Float) => true,
            (Type::String, Type::String) => true,
            (Type::Bool, Type::Bool) => true,
            (Type::Nil, Type::Nil) => true,
            (Type::List(a), Type::List(e)) => self.types_compatible(a, e),
            (Type::Dict(ak, av), Type::Dict(ek, ev)) => {
                self.types_compatible(ak, ek) && self.types_compatible(av, ev)
            }
            (Type::Tuple(a), Type::Tuple(e)) => {
                a.len() == e.len() && a.iter().zip(e.iter()).all(|(x, y)| self.types_compatible(x, y))
            }
            (Type::Range, Type::Range) => true,
            (Type::Instance(a), Type::Instance(e)) => a == e,
            (Type::Result(oka, erra), Type::Result(oke, erre)) => {
                self.types_compatible(oka, oke) && self.types_compatible(erra, erre)
            }
            (Type::Option(a), Type::Option(e)) => self.types_compatible(a, e),
            (Type::TraitObject(trait_name), Type::Instance(_type_name)) => {
                // Check if type has an impl for the trait
                self.traits.contains_key(trait_name) // accept any Instance if trait exists
            }
            (Type::Instance(_), Type::TraitObject(_)) => true,
            (Type::TraitObject(_), Type::TraitObject(_)) => true,
            (Type::Generic(_), _) | (_, Type::Generic(_)) => true,
            _ => false,
        }
    }
}
