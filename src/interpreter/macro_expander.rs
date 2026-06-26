use crate::ast::*;

pub struct MacroDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
}

pub struct MacroExpander {
    macros: Vec<MacroDef>,
}

impl MacroExpander {
    pub fn new() -> Self {
        MacroExpander { macros: Vec::new() }
    }

    pub fn register(&mut self, name: String, params: Vec<String>, body: Vec<Stmt>) {
        self.macros.push(MacroDef { name, params, body });
    }

    pub fn has_macros(&self) -> bool {
        !self.macros.is_empty()
    }

    pub fn expand(&self, stmts: &[Stmt]) -> Result<Vec<Stmt>, String> {
        let mut result = Vec::new();
        for stmt in stmts {
            if matches!(stmt, Stmt::Macro { .. }) {
                continue;
            }
            match self.try_expand_stmt(stmt, 0)? {
                Some(expanded) => result.extend(expanded),
                None => result.push(stmt.clone()),
            }
        }
        Ok(result)
    }

    fn try_expand_stmt(&self, stmt: &Stmt, depth: usize) -> Result<Option<Vec<Stmt>>, String> {
        if depth > 10 {
            return Err("Macro expansion recursion depth exceeded".into());
        }
        match stmt {
            Stmt::Expr { expr, span: _ } => {
                if let Expr::Call { callee, args } = expr.as_ref() {
                    if let Some(def) = self.macros.iter().find(|m| m.name == *callee) {
                        if args.len() != def.params.len() {
                            return Err(format!("Macro '{}' expects {} arguments, got {}", callee, def.params.len(), args.len()));
                        }
                        let expanded = self.substitute(&def.body, args, &def.params);
                        Ok(Some(expanded))
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    fn substitute(&self, template: &[Stmt], args: &[Expr], params: &[String]) -> Vec<Stmt> {
        template.iter()
            .map(|s| self.substitute_stmt(s, args, params))
            .collect()
    }

    fn substitute_stmt(&self, stmt: &Stmt, args: &[Expr], params: &[String]) -> Stmt {
        match stmt {
            Stmt::Let { span, pub_flag, name, type_ann, value } => {
                Stmt::Let {
                    span: span.clone(),
                    pub_flag: *pub_flag,
                    name: name.clone(),
                    type_ann: type_ann.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Stmt::Struct { span, pub_flag, name, fields } => {
                Stmt::Struct { span: span.clone(), pub_flag: *pub_flag, name: name.clone(), fields: fields.clone() }
            }
            Stmt::Enum { span, pub_flag, name, variants } => {
                Stmt::Enum { span: span.clone(), pub_flag: *pub_flag, name: name.clone(), variants: variants.clone() }
            }
            Stmt::Fn { span, pub_flag, name, generic_params, params: fn_params, return_type, body, is_async } => {
                Stmt::Fn {
                    span: span.clone(),
                    pub_flag: *pub_flag,
                    name: name.clone(),
                    generic_params: generic_params.clone(),
                    params: fn_params.clone(),
                    return_type: return_type.clone(),
                    body: body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                    is_async: *is_async,
                }
            }
            Stmt::If { span, condition, then_branch, else_branch } => {
                Stmt::If {
                    span: span.clone(),
                    condition: Box::new(self.substitute_expr(condition, args, params)),
                    then_branch: then_branch.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                    else_branch: else_branch.as_ref().map(|b| b.iter().map(|s| self.substitute_stmt(s, args, params)).collect()),
                }
            }
            Stmt::While { span, condition, body } => {
                Stmt::While {
                    span: span.clone(),
                    condition: Box::new(self.substitute_expr(condition, args, params)),
                    body: body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                }
            }
            Stmt::For { span, var, iterable, body } => {
                Stmt::For {
                    span: span.clone(),
                    var: var.clone(),
                    iterable: Box::new(self.substitute_expr(iterable, args, params)),
                    body: body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                }
            }
            Stmt::Macro { .. } => stmt.clone(),
            Stmt::Break { span } => Stmt::Break { span: span.clone() },
            Stmt::Continue { span } => Stmt::Continue { span: span.clone() },
            Stmt::DoWhile { span, body, condition } => {
                Stmt::DoWhile {
                    span: span.clone(),
                    body: body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                    condition: Box::new(self.substitute_expr(condition, args, params)),
                }
            }
            Stmt::Destructure { span, pub_flag, target, value } => {
                Stmt::Destructure {
                    span: span.clone(),
                    pub_flag: *pub_flag,
                    target: target.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Stmt::Match { span, value, arms } => {
                Stmt::Match {
                    span: span.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                    arms: arms.iter().map(|arm| {
                        MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: arm.guard.as_ref().map(|g| self.substitute_expr(g, args, params)),
                            body: arm.body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                        }
                    }).collect(),
                }
            }
            Stmt::Try { span, body, catch_var, catch_body } => {
                Stmt::Try {
                    span: span.clone(),
                    body: body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                    catch_var: catch_var.clone(),
                    catch_body: catch_body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                }
            }
            Stmt::Return { span, value } => {
                Stmt::Return {
                    span: span.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Stmt::Yield { span, value } => {
                Stmt::Yield {
                    span: span.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Stmt::Throw { span, value } => {
                Stmt::Throw {
                    span: span.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Stmt::Print { span, value, newline } => {
                Stmt::Print {
                    span: span.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                    newline: *newline,
                }
            }
            Stmt::Import { span, pub_flag, path, alias } => {
                Stmt::Import { span: span.clone(), pub_flag: *pub_flag, path: path.clone(), alias: alias.clone() }
            }
            Stmt::Class { span, pub_flag, name, extends, methods } => {
                Stmt::Class {
                    span: span.clone(),
                    pub_flag: *pub_flag,
                    name: name.clone(),
                    extends: extends.clone(),
                    methods: methods.iter().map(|m| {
                        let mut m2 = m.clone();
                        m2.body = m2.body.iter().map(|s| self.substitute_stmt(s, args, params)).collect();
                        m2
                    }).collect(),
                }
            }
            Stmt::Trait { span, pub_flag, name, methods } => {
                Stmt::Trait { span: span.clone(), pub_flag: *pub_flag, name: name.clone(), methods: methods.clone() }
            }
            Stmt::Impl { span, trait_name, type_name, methods } => {
                Stmt::Impl {
                    span: span.clone(),
                    trait_name: trait_name.clone(),
                    type_name: type_name.clone(),
                    methods: methods.iter().map(|m| {
                        let mut m2 = m.clone();
                        m2.body = m2.body.iter().map(|s| self.substitute_stmt(s, args, params)).collect();
                        m2
                    }).collect(),
                }
            }
            Stmt::Expr { span, expr } => {
                Stmt::Expr {
                    span: span.clone(),
                    expr: Box::new(self.substitute_expr(expr, args, params)),
                }
            }
        }
    }

    fn substitute_expr(&self, expr: &Expr, args: &[Expr], params: &[String]) -> Expr {
        match expr {
            Expr::Variable(name) => {
                if let Some(stripped) = name.strip_prefix('$') {
                    if let Some(idx) = params.iter().position(|p| p == stripped) {
                        return args[idx].clone();
                    }
                }
                expr.clone()
            }
            Expr::Int(_) | Expr::Float(_) | Expr::String(_) | Expr::Boolean(_) | Expr::Nil => {
                expr.clone()
            }
            Expr::Fn { generic_params, params: fn_params, body } => {
                Expr::Fn {
                    generic_params: generic_params.clone(),
                    params: fn_params.clone(),
                    body: body.iter().map(|s| self.substitute_stmt(s, args, params)).collect(),
                }
            }
            Expr::List(items) => {
                Expr::List(items.iter().map(|i| self.substitute_expr(i, args, params)).collect())
            }
            Expr::Set(items) => {
                Expr::Set(items.iter().map(|i| self.substitute_expr(i, args, params)).collect())
            }
            Expr::Tuple(items) => {
                Expr::Tuple(items.iter().map(|i| self.substitute_expr(i, args, params)).collect())
            }
            Expr::Dict(pairs) => {
                Expr::Dict(pairs.iter().map(|(k, v)| (self.substitute_expr(k, args, params), self.substitute_expr(v, args, params))).collect())
            }
            Expr::Index { object, index } => {
                Expr::Index {
                    object: Box::new(self.substitute_expr(object, args, params)),
                    index: Box::new(self.substitute_expr(index, args, params)),
                }
            }
            Expr::Slice { object, start, end } => {
                Expr::Slice {
                    object: Box::new(self.substitute_expr(object, args, params)),
                    start: start.as_ref().map(|e| Box::new(self.substitute_expr(e, args, params))),
                    end: end.as_ref().map(|e| Box::new(self.substitute_expr(e, args, params))),
                }
            }
            Expr::Spread(inner) => {
                Expr::Spread(Box::new(self.substitute_expr(inner, args, params)))
            }
            Expr::Range { start, end } => {
                Expr::Range {
                    start: Box::new(self.substitute_expr(start, args, params)),
                    end: Box::new(self.substitute_expr(end, args, params)),
                }
            }
            Expr::BinaryOp { left, op, right } => {
                Expr::BinaryOp {
                    left: Box::new(self.substitute_expr(left, args, params)),
                    op: op.clone(),
                    right: Box::new(self.substitute_expr(right, args, params)),
                }
            }
            Expr::UnaryOp { op, right } => {
                Expr::UnaryOp {
                    op: op.clone(),
                    right: Box::new(self.substitute_expr(right, args, params)),
                }
            }
            Expr::Call { callee, args: call_args } => {
                Expr::Call {
                    callee: callee.clone(),
                    args: call_args.iter().map(|a| self.substitute_expr(a, args, params)).collect(),
                }
            }
            Expr::MethodCall { object, method, args: call_args } => {
                Expr::MethodCall {
                    object: Box::new(self.substitute_expr(object, args, params)),
                    method: method.clone(),
                    args: call_args.iter().map(|a| self.substitute_expr(a, args, params)).collect(),
                }
            }
            Expr::Assignment { name, value } => {
                Expr::Assignment {
                    name: name.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Expr::CompoundAssign { name, op, value } => {
                Expr::CompoundAssign {
                    name: name.clone(),
                    op: op.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Expr::Grouping(inner) => {
                Expr::Grouping(Box::new(self.substitute_expr(inner, args, params)))
            }
            Expr::StringInterp(parts) => {
                Expr::StringInterp(parts.iter().map(|p| self.substitute_expr(p, args, params)).collect())
            }
            Expr::FieldAccess { object, field } => {
                Expr::FieldAccess {
                    object: Box::new(self.substitute_expr(object, args, params)),
                    field: field.clone(),
                }
            }
            Expr::Await { value } => {
                Expr::Await { value: Box::new(self.substitute_expr(value, args, params)) }
            }
            Expr::FieldAssign { object, field, value } => {
                Expr::FieldAssign {
                    object: Box::new(self.substitute_expr(object, args, params)),
                    field: field.clone(),
                    value: Box::new(self.substitute_expr(value, args, params)),
                }
            }
            Expr::Ternary { condition, then_expr, else_expr } => {
                Expr::Ternary {
                    condition: Box::new(self.substitute_expr(condition, args, params)),
                    then_expr: Box::new(self.substitute_expr(then_expr, args, params)),
                    else_expr: Box::new(self.substitute_expr(else_expr, args, params)),
                }
            }
            Expr::Try { expr: inner } => {
                Expr::Try { expr: Box::new(self.substitute_expr(inner, args, params)) }
            }
            Expr::Super => Expr::Super,
            Expr::ListComp { expr, clauses } => Expr::ListComp {
                expr: Box::new(self.substitute_expr(expr, args, params)),
                clauses: clauses.iter().map(|c| CompClause {
                    var: c.var.clone(),
                    iterable: Box::new(self.substitute_expr(&c.iterable, args, params)),
                    conditions: c.conditions.iter().map(|cond| self.substitute_expr(cond, args, params)).collect(),
                }).collect(),
            },
            Expr::SetComp { expr, clauses } => Expr::SetComp {
                expr: Box::new(self.substitute_expr(expr, args, params)),
                clauses: clauses.iter().map(|c| CompClause {
                    var: c.var.clone(),
                    iterable: Box::new(self.substitute_expr(&c.iterable, args, params)),
                    conditions: c.conditions.iter().map(|cond| self.substitute_expr(cond, args, params)).collect(),
                }).collect(),
            },
            Expr::DictComp { key, value, clauses } => Expr::DictComp {
                key: Box::new(self.substitute_expr(key, args, params)),
                value: Box::new(self.substitute_expr(value, args, params)),
                clauses: clauses.iter().map(|c| CompClause {
                    var: c.var.clone(),
                    iterable: Box::new(self.substitute_expr(&c.iterable, args, params)),
                    conditions: c.conditions.iter().map(|cond| self.substitute_expr(cond, args, params)).collect(),
                }).collect(),
            },
        }
    }
}
