use crate::ast::{BinaryOpKind, DestructureItem, DestructureTarget, Expr, MatchArm, MatchPattern, Stmt, Type, UnaryOpKind};

pub struct Formatter {
    output: String,
    indent: usize,
    pending_newline: bool,
}

impl Formatter {
    pub fn new() -> Self {
        Formatter {
            output: String::new(),
            indent: 0,
            pending_newline: false,
        }
    }

    pub fn format(&mut self, stmts: &[Stmt]) -> String {
        for (i, stmt) in stmts.iter().enumerate() {
            if i > 0 {
                self.write_newline();
            }
            self.format_stmt(stmt);
        }
        self.write_newline();
        std::mem::take(&mut self.output)
    }

    fn write(&mut self, s: &str) {
        if self.pending_newline {
            self.output.push('\n');
            for _ in 0..self.indent {
                self.output.push(' ');
                self.output.push(' ');
                self.output.push(' ');
                self.output.push(' ');
            }
            self.pending_newline = false;
        }
        self.output.push_str(s);
    }

    fn write_newline(&mut self) {
        if !self.pending_newline {
            self.pending_newline = true;
        }
    }

    fn indent_inc(&mut self) {
        self.indent += 1;
    }

    fn indent_dec(&mut self) {
        self.indent = self.indent.saturating_sub(1);
    }

    fn format_block(&mut self, stmts: &[Stmt]) {
        self.indent_inc();
        for stmt in stmts {
            self.write_newline();
            self.format_stmt(stmt);
        }
        self.indent_dec();
    }

    fn format_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                span: _,
                pub_flag: _,
                name,
                type_ann,
                value,
            } => {
                self.write("cell ");
                self.write(name);
                if let Some(t) = type_ann {
                    self.write(": ");
                    self.format_type(t);
                }
                self.write(" = ");
                self.format_expr(value);
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
                if *is_async {
                    self.write("async ");
                }
                self.write("~");
                self.write(name);
                self.format_generic_params(generic_params);
                self.write("(");
                for (i, (pn, pt, pd)) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(pn);
                    if let Some(t) = pt {
                        self.write(": ");
                        self.format_type(t);
                    }
                    if let Some(d) = pd {
                        self.write(" = ");
                        self.format_expr(d);
                    }
                }
                self.write(")");
                if let Some(rt) = return_type {
                    self.write(" -> ");
                    self.format_type(rt);
                }
                self.write_newline();
                self.format_block(body);
            }
            Stmt::If {
                span: _,
                condition,
                then_branch,
                else_branch,
            } => {
                self.write("when ");
                self.format_expr(condition);
                self.write_newline();
                self.format_block(then_branch);
                if let Some(eb) = else_branch {
                    if eb.len() == 1 && matches!(eb[0], Stmt::If { .. }) {
                        self.write_newline();
                        self.write("else when ");
                        self.format_stmt(&eb[0]);
                    } else {
                        self.write_newline();
                        self.write("else");
                        self.write_newline();
                        self.format_block(eb);
                    }
                }
            }
            Stmt::While { span: _, condition, body } => {
                self.write("while ");
                self.format_expr(condition);
                self.write_newline();
                self.format_block(body);
            }
            Stmt::DoWhile { span: _, condition, body } => {
                self.write("do");
                self.write_newline();
                self.format_block(body);
                self.write("while ");
                self.format_expr(condition);
            }
            Stmt::Destructure { span: _, pub_flag: _, target, value } => {
                self.write("cell ");
                match target {
                    DestructureTarget::List(items) => {
                        self.write("(");
                        for (i, item) in items.iter().enumerate() {
                            if i > 0 { self.write(", "); }
                            match item {
                                DestructureItem::Name(n) => self.write(n),
                                DestructureItem::Rest(n) => {
                                    self.write("...");
                                    self.write(n);
                                }
                            }
                        }
                        self.write(")");
                    }
                    DestructureTarget::Struct(fields) => {
                        self.write("{");
                        for (i, f) in fields.iter().enumerate() {
                            if i > 0 { self.write(", "); }
                            self.write(f);
                        }
                        self.write("}");
                    }
                }
                self.write(" = ");
                self.format_expr(value);
            }
            Stmt::For {
                span: _,
                var,
                iterable,
                body,
            } => {
                self.write("over ");
                self.write(var);
                self.write(" in ");
                self.format_expr(iterable);
                self.write_newline();
                self.format_block(body);
            }
            Stmt::Break { .. } => {
                self.write("break");
            }
            Stmt::Continue { .. } => {
                self.write("continue");
            }
            Stmt::Match { span: _, value, arms } => {
                self.write("pick ");
                self.format_expr(value);
                self.write_newline();
                self.indent_inc();
                for arm in arms {
                    self.write_newline();
                    self.format_match_arm(arm);
                }
                self.indent_dec();
            }
            Stmt::Try {
                span: _,
                body,
                catch_var,
                catch_body,
            } => {
                self.write("dare");
                self.write_newline();
                self.format_block(body);
                self.write_newline();
                self.write("catch ");
                self.write(catch_var);
                self.write_newline();
                self.format_block(catch_body);
            }
            Stmt::Return { span: _, value } => {
                if matches!(value.as_ref(), Expr::Nil) {
                    self.write("emit");
                } else {
                    self.write("emit ");
                    self.format_expr(value);
                }
            }
            Stmt::Print { span: _, value, newline } => {
                if *newline {
                    self.write("shout");
                } else {
                    self.write("say");
                }
                let needs_space = !matches!(value.as_ref(), Expr::Grouping(_));
                if needs_space {
                    self.write(" ");
                }
                self.format_expr(value);
            }
            Stmt::Import { span: _, pub_flag: _, path, alias } => {
                self.write("load \"");
                self.write(path);
                self.write("\"");
                if let Some(a) = alias {
                    self.write(" as ");
                    self.write(a);
                }
            }
            Stmt::Struct { span: _, pub_flag: _, name, fields } => {
                self.write("shape ");
                self.write(name);
                if !fields.is_empty() {
                    self.write_newline();
                    self.indent_inc();
                    for (fname, ftype) in fields {
                        self.write_newline();
                        self.write(fname);
                        if let Some(t) = ftype {
                            self.write(": ");
                            self.format_type(t);
                        }
                    }
                    self.indent_dec();
                }
            }
            Stmt::Enum { span: _, pub_flag: _, name, variants } => {
                self.write("style ");
                self.write(name);
                if !variants.is_empty() {
                    self.write_newline();
                    self.indent_inc();
                    for variant in variants {
                        self.write_newline();
                        self.write(&variant.name);
                        if !variant.fields.is_empty() {
                            self.write_newline();
                            self.indent_inc();
                            for (fname, ftype) in &variant.fields {
                                self.write_newline();
                                self.write(fname);
                                if let Some(t) = ftype {
                                    self.write(": ");
                                    self.format_type(t);
                                }
                            }
                            self.indent_dec();
                        }
                    }
                    self.indent_dec();
                }
            }
            Stmt::Class { span: _, pub_flag: _, name, extends, methods } => {
                self.write("class ");
                self.write(name);
                if let Some(parent) = extends {
                    self.write(" extends ");
                    self.write(parent);
                }
                if !methods.is_empty() {
                    self.write_newline();
                    self.indent_inc();
                    for m in methods {
                        self.write_newline();
                        self.write("~");
                        self.write(&m.name);
                        self.write("(");
                        for (i, (pn, pt, pd)) in m.params.iter().enumerate() {
                            if i > 0 { self.write(", "); }
                            self.write(pn);
                            if let Some(t) = pt {
                                self.write(": ");
                                self.format_type(t);
                            }
                            if let Some(d) = pd {
                                self.write(" = ");
                                self.format_expr(d);
                            }
                        }
                        self.write(")");
                        self.write_newline();
                        self.format_block(&m.body);
                    }
                    self.indent_dec();
                }
            }
            Stmt::Expr { span: _, expr } => {
                self.format_expr(expr);
            }
            Stmt::Yield { span: _, value } => {
                self.write("yield ");
                self.format_expr(value);
            }
            Stmt::Throw { span: _, value } => {
                self.write("throw ");
                self.format_expr(value);
            }
            Stmt::Trait { span: _, pub_flag: _, name, methods } => {
                self.write("trait ");
                self.write(name);
                if !methods.is_empty() {
                    self.write_newline();
                    self.indent_inc();
                    for m in methods {
                        self.write_newline();
                        self.write("~");
                        self.write(&m.name);
                        self.write("(");
                        for (i, (pn, pt, pd)) in m.params.iter().enumerate() {
                            if i > 0 { self.write(", "); }
                            self.write(pn);
                            if let Some(t) = pt {
                                self.write(": ");
                                self.format_type(t);
                            }
                            if let Some(d) = pd {
                                self.write(" = ");
                                self.format_expr(d);
                            }
                        }
                        self.write(")");
                        if let Some(rt) = &m.return_type {
                            self.write(" -> ");
                            self.format_type(rt);
                        }
                        if let Some(body) = &m.body {
                            self.write_newline();
                            self.format_block(body);
                        }
                    }
                    self.indent_dec();
                }
            }
            Stmt::Macro { .. } => {}
            Stmt::Impl { span: _, trait_name, type_name, methods } => {
                self.write("impl ");
                self.write(trait_name);
                self.write(" for ");
                self.write(type_name);
                if !methods.is_empty() {
                    self.write_newline();
                    self.indent_inc();
                    for m in methods {
                        self.write_newline();
                        self.write("~");
                        self.write(&m.name);
                        self.write("(");
                        for (i, (pn, pt, pd)) in m.params.iter().enumerate() {
                            if i > 0 { self.write(", "); }
                            self.write(pn);
                            if let Some(t) = pt {
                                self.write(": ");
                                self.format_type(t);
                            }
                            if let Some(d) = pd {
                                self.write(" = ");
                                self.format_expr(d);
                            }
                        }
                        self.write(")");
                        if let Some(rt) = &m.return_type {
                            self.write(" -> ");
                            self.format_type(rt);
                        }
                        self.write_newline();
                        self.format_block(&m.body);
                    }
                    self.indent_dec();
                }
            }
        }
    }

    fn format_match_arm(&mut self, arm: &MatchArm) {
        self.format_match_pattern(&arm.pattern);
        if let Some(guard) = &arm.guard {
            self.write(" if ");
            self.format_expr(guard);
        }
        if arm.body.len() == 1 && matches!(&arm.body[0], Stmt::Expr { .. }) {
            self.write(" -> ");
            if let Stmt::Expr { span: _, expr } = &arm.body[0] {
                self.format_expr(expr);
            }
        } else {
            self.write(" ->");
            self.write_newline();
            self.indent_inc();
            for s in &arm.body {
                self.write_newline();
                self.format_stmt(s);
            }
            self.indent_dec();
        }
    }

    fn format_match_pattern(&mut self, pattern: &MatchPattern) {
        match pattern {
            MatchPattern::Literal(expr) => self.format_expr(expr),
            MatchPattern::Wildcard => self.write("_"),
            MatchPattern::Binding(name) => self.write(name),
            MatchPattern::Destructure(name, fields) => {
                self.write(name);
                self.write("(");
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(f);
                }
                self.write(")");
            }
            MatchPattern::Or(patterns) => {
                for (i, p) in patterns.iter().enumerate() {
                    if i > 0 {
                        self.write(" | ");
                    }
                    self.format_match_pattern(p);
                }
            }
        }
    }

    fn format_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Int(n) => {
                self.write(&n.to_string());
            }
            Expr::Float(n) => {
                if n.fract() == 0.0 && n.is_finite() {
                    self.write(&format!("{}", *n as i64));
                } else {
                    self.write(&n.to_string());
                }
            }
            Expr::String(s) => {
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t")
                    .replace('\r', "\\r");
                self.write(&format!("\"{}\"", escaped));
            }
            Expr::Boolean(b) => {
                self.write(if *b { "yes" } else { "no" });
            }
            Expr::Nil => self.write("none"),
            Expr::Variable(name) => self.write(name),
            Expr::Fn {
                generic_params,
                params,
                body,
            } => {
                self.write("\\");
                self.format_generic_params(generic_params);
                self.write("(");
                for (i, (pn, pt, pd)) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(pn);
                    if let Some(t) = pt {
                        self.write(": ");
                        self.format_type(t);
                    }
                    if let Some(d) = pd {
                        self.write(" = ");
                        self.format_expr(d);
                    }
                }
                self.write(")");
                if body.len() == 1 && matches!(&body[0], Stmt::Expr { .. }) {
                    if let Stmt::Expr { span: _, expr } = &body[0] {
                        self.write(" ");
                        self.format_expr(expr);
                    }
                } else {
                    self.write_newline();
                    self.format_block(body);
                }
            }
            Expr::List(items) => {
                self.write("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(item);
                }
                self.write("]");
            }
            Expr::Tuple(items) => {
                self.write("(");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(item);
                }
                if items.len() == 1 {
                    self.write(",");
                }
                self.write(")");
            }
            Expr::Set(items) => {
                self.write("{");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(item);
                }
                self.write("}");
            }
            Expr::Dict(pairs) => {
                self.write("{");
                for (i, (k, v)) in pairs.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(k);
                    self.write(": ");
                    self.format_expr(v);
                }
                self.write("}");
            }
            Expr::Index { object, index } => {
                self.format_expr(object);
                self.write("[");
                self.format_expr(index);
                self.write("]");
            }
            Expr::Range { start, end } => {
                self.format_expr(start);
                self.write("..");
                self.format_expr(end);
            }
            Expr::Slice { object, start, end } => {
                self.format_expr(object);
                self.write("[");
                if let Some(s) = start {
                    self.format_expr(s);
                }
                self.write(":");
                if let Some(e) = end {
                    self.format_expr(e);
                }
                self.write("]");
            }
            Expr::Spread(inner) => {
                self.write("...");
                self.format_expr(inner);
            }
            Expr::BinaryOp { left, op, right } => {
                self.format_expr(left);
                self.write(" ");
                self.write(match op {
                    BinaryOpKind::Add => "+",
                    BinaryOpKind::Subtract => "-",
                    BinaryOpKind::Multiply => "*",
                    BinaryOpKind::Divide => "/",
                    BinaryOpKind::Modulo => "%",
                    BinaryOpKind::Equal => "==",
                    BinaryOpKind::NotEqual => "!=",
                    BinaryOpKind::Less => "<",
                    BinaryOpKind::LessEqual => "<=",
                    BinaryOpKind::Greater => ">",
                    BinaryOpKind::GreaterEqual => ">=",
                    BinaryOpKind::And => "&&",
                    BinaryOpKind::Or => "||",
                    BinaryOpKind::In => "in",
                    BinaryOpKind::BitAnd => "&",
                    BinaryOpKind::BitOr => "|",
                    BinaryOpKind::BitXor => "^",
                    BinaryOpKind::ShiftLeft => "<<",
                    BinaryOpKind::ShiftRight => ">>",
                });
                self.write(" ");
                self.format_expr(right);
            }
            Expr::UnaryOp { op, right } => {
                match op {
                    UnaryOpKind::Negate => self.write("-"),
                    UnaryOpKind::Not => self.write("!"),
                    UnaryOpKind::BitNot => self.write("~"),
                }
                self.format_expr(right);
            }
            Expr::Call { callee, args } => {
                self.write(callee);
                self.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(arg);
                }
                self.write(")");
            }
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                self.format_expr(object);
                self.write(".");
                self.write(method);
                self.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expr(arg);
                }
                self.write(")");
            }
            Expr::Assignment { name, value } => {
                self.write(name);
                self.write(" = ");
                self.format_expr(value);
            }
            Expr::CompoundAssign { name, op, value } => {
                self.write(name);
                self.write(" ");
                self.write(match op {
                    BinaryOpKind::Add => "+=",
                    BinaryOpKind::Subtract => "-=",
                    BinaryOpKind::Multiply => "*=",
                    BinaryOpKind::Divide => "/=",
                    BinaryOpKind::Modulo => "%=",
                    _ => "=",
                });
                self.write(" ");
                self.format_expr(value);
            }
            Expr::Grouping(inner) => {
                self.write("(");
                self.format_expr(inner);
                self.write(")");
            }
            Expr::StringInterp(parts) => {
                self.write("\"");
                for part in parts {
                    match part {
                        Expr::String(s) => {
                            let escaped = s
                                .replace('\\', "\\\\")
                                .replace('"', "\\\"")
                                .replace('\n', "\\n")
                                .replace('\t', "\\t")
                                .replace('\r', "\\r");
                            self.write(&escaped);
                        }
                        other => {
                            self.write("{");
                            self.format_expr(other);
                            self.write("}");
                        }
                    }
                }
                self.write("\"");
            }
            Expr::FieldAccess { object, field } => {
                self.format_expr(object);
                self.write(".");
                self.write(field);
            }
            Expr::FieldAssign { object, field, value } => {
                self.format_expr(object);
                self.write(".");
                self.write(field);
                self.write(" = ");
                self.format_expr(value);
            }
            Expr::Await { value } => {
                self.write("await ");
                self.format_expr(value);
            }
            Expr::Ternary { condition, then_expr, else_expr } => {
                self.format_expr(condition);
                self.write(" ? ");
                self.format_expr(then_expr);
                self.write(" : ");
                self.format_expr(else_expr);
            }
            Expr::Try { expr } => {
                self.format_expr(expr);
                self.write("?");
            }
            Expr::Super => {
                self.write("super");
            }
            Expr::ListComp { expr, clauses } => {
                self.write("[");
                self.format_expr(expr);
                for c in clauses {
                    self.write(" for ");
                    self.write(&c.var);
                    self.write(" in ");
                    self.format_expr(&c.iterable);
                    for cond in &c.conditions {
                        self.write(" when ");
                        self.format_expr(cond);
                    }
                }
                self.write("]");
            }
            Expr::SetComp { expr, clauses } => {
                self.write("{");
                self.format_expr(expr);
                for c in clauses {
                    self.write(" for ");
                    self.write(&c.var);
                    self.write(" in ");
                    self.format_expr(&c.iterable);
                    for cond in &c.conditions {
                        self.write(" when ");
                        self.format_expr(cond);
                    }
                }
                self.write("}");
            }
            Expr::DictComp { key, value, clauses } => {
                self.write("{");
                self.format_expr(key);
                self.write(": ");
                self.format_expr(value);
                for c in clauses {
                    self.write(" for ");
                    self.write(&c.var);
                    self.write(" in ");
                    self.format_expr(&c.iterable);
                    for cond in &c.conditions {
                        self.write(" when ");
                        self.format_expr(cond);
                    }
                }
                self.write("}");
            }
        }
    }

    fn format_type(&mut self, t: &Type) {
        match t {
            Type::Int => self.write("int"),
            Type::Float => self.write("float"),
            Type::String => self.write("string"),
            Type::Bool => self.write("bool"),
            Type::List(inner) => {
                self.write("list<");
                self.format_type(inner);
                self.write(">");
            }
            Type::Dict(k, v) => {
                self.write("dict<");
                self.format_type(k);
                self.write(", ");
                self.format_type(v);
                self.write(">");
            }
            Type::Fn(params, ret) => {
                self.write("\\(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_type(p);
                }
                self.write(") -> ");
                self.format_type(ret);
            }
            Type::Tuple(ts) => {
                self.write("(");
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_type(t);
                }
                if ts.len() == 1 {
                    self.write(",");
                }
                self.write(")");
            }
            Type::Range => self.write("range"),
            Type::Nil => self.write("none"),
            Type::Result(ok, err) => {
                self.write("Result<");
                self.format_type(ok);
                self.write(", ");
                self.format_type(err);
                self.write(">");
            }
            Type::Option(inner) => {
                self.write("Option<");
                self.format_type(inner);
                self.write(">");
            }
            Type::Any => self.write("any"),
            Type::SelfType => self.write("Self"),
            Type::Instance(name) => self.write(name),
            Type::Generic(name) => self.write(name),
            Type::TraitObject(name) => {
                self.write("impl ");
                self.write(name);
            }
        }
    }

    fn format_generic_params(&mut self, params: &[String]) {
        if params.is_empty() {
            return;
        }
        self.write("<");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(p);
        }
        self.write(">");
    }
}

pub fn format_source(source: &str) -> Result<String, String> {
    let tokens = crate::lexer::Lexer::new(source).tokenize();
    let stmts = crate::parser::Parser::new(tokens).parse()?;
    let mut fmt = Formatter::new();
    Ok(fmt.format(&stmts))
}
