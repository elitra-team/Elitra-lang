use crate::ast::{BinaryOpKind, DestructureItem, DestructureTarget, Expr, HasSpan, MatchArm, MatchPattern, SourceSpan, Stmt, TraitMethod, TraitMethodImpl, Type, UnaryOpKind};
use crate::token::{Token, TokenType};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            self.skip_newlines_and_semicolons();
            if self.is_at_end() {
                break;
            }
            if self.check_exact(&TokenType::Dedent) {
                break;
            }
            match self.parse_statement() {
                Ok(s) => statements.push(s),
                Err(e) => {
                    self.errors.push(e);
                    self.sync_to_statement_boundary();
                }
            }
        }
        if self.errors.is_empty() {
            Ok(statements)
        } else {
            Err(self.errors.join("\n"))
        }
    }

    fn sync_to_statement_boundary(&mut self) {
        while !self.is_at_end() {
            match &self.peek().token_type {
                TokenType::Newline | TokenType::Semicolon => {
                    self.advance();
                    return;
                }
                TokenType::Dedent | TokenType::Eof => {
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }
    fn previous(&self) -> &Token {
        &self.tokens[self.pos - 1]
    }
    fn is_at_end(&self) -> bool {
        self.peek().token_type == TokenType::Eof
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.pos += 1;
        }
        &self.tokens[self.pos - 1]
    }

    fn check_exact(&self, tt: &TokenType) -> bool {
        self.peek().token_type == *tt
    }

    fn match_any(&mut self, types: &[TokenType]) -> bool {
        for t in types {
            if self.check_exact(t) {
                self.advance();
                return true;
            }
        }
        false
    }

    fn expect(&mut self, tt: &TokenType, msg: &str) -> Result<(), String> {
        if self.check_exact(tt) {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Line {}: {} - expected {}, got {}",
                self.peek().line,
                msg,
                tt,
                self.peek().token_type
            ))
        }
    }

    fn skip_newlines_and_semicolons(&mut self) {
        while self.check_exact(&TokenType::Newline) || self.check_exact(&TokenType::Semicolon) {
            self.advance();
        }
    }

    fn parse_type(&mut self) -> Option<Type> {
        if !self.check_exact(&TokenType::Colon) {
            return None;
        }
        self.advance();
        Some(self.parse_type_annotation())
    }

    fn parse_type_annotation(&mut self) -> Type {
        let tok = self.peek().clone();
        match &tok.token_type {
            TokenType::TypeInt => {
                self.advance();
                Type::Int
            }
            TokenType::TypeFloat => {
                self.advance();
                Type::Float
            }
            TokenType::TypeString => {
                self.advance();
                Type::String
            }
            TokenType::TypeBool => {
                self.advance();
                Type::Bool
            }
            TokenType::TypeList => {
                self.advance();
                if self.check_exact(&TokenType::Less) {
                    self.advance();
                    let inner = self.parse_type_annotation();
                    let _ = self.expect(&TokenType::Greater, "Expected '>'");
                    Type::List(Box::new(inner))
                } else {
                    Type::List(Box::new(Type::Any))
                }
            }
            TokenType::TypeSelf => {
                self.advance();
                Type::SelfType
            }
            TokenType::TypeDict => {
                self.advance();
                if self.check_exact(&TokenType::Less) {
                    self.advance();
                    let kt = self.parse_type_annotation();
                    let _ = self.expect(&TokenType::Comma, "Expected ',' in dict type");
                    let vt = self.parse_type_annotation();
                    let _ = self.expect(&TokenType::Greater, "Expected '>'");
                    Type::Dict(Box::new(kt), Box::new(vt))
                } else {
                    Type::Dict(Box::new(Type::Any), Box::new(Type::Any))
                }
            }
            TokenType::Identifier(name) => {
                self.advance();
                match name.as_str() {
                    "Result" => {
                        if self.check_exact(&TokenType::Less) {
                            self.advance();
                            let ok_t = self.parse_type_annotation();
                            let _ = self.expect(&TokenType::Comma, "Expected ',' in Result<Ok, Err>");
                            let err_t = self.parse_type_annotation();
                            let _ = self.expect(&TokenType::Greater, "Expected '>'");
                            Type::Result(Box::new(ok_t), Box::new(err_t))
                        } else {
                            Type::Generic("Result".into())
                        }
                    }
                    "Option" => {
                        if self.check_exact(&TokenType::Less) {
                            self.advance();
                            let inner = self.parse_type_annotation();
                            let _ = self.expect(&TokenType::Greater, "Expected '>'");
                            Type::Option(Box::new(inner))
                        } else {
                            Type::Generic("Option".into())
                        }
                    }
                    _ => Type::Generic(name.clone()),
                }
            }
            _ => Type::Any,
        }
    }

    fn parse_return_type(&mut self) -> Option<Type> {
        if self.check_exact(&TokenType::Arrow) {
            self.advance();
            Some(self.parse_type_annotation())
        } else {
            None
        }
    }

    fn parse_params(&mut self) -> Result<Vec<(String, Option<Type>, Option<Expr>)>, String> {
        let mut params = Vec::new();
        if !self.check_exact(&TokenType::RParen) {
            loop {
                if self.check_exact(&TokenType::RParen) {
                    break;
                }
                let tok = self.advance();
                let pname = match &tok.token_type {
                    TokenType::Identifier(n) => n.clone(),
                    t => {
                        return Err(format!(
                            "Line {}: expected parameter name, got {}",
                            tok.line, t
                        ))
                    }
                };
                let ptype = if self.check_exact(&TokenType::Colon) {
                    self.advance();
                    Some(self.parse_type_annotation())
                } else {
                    None
                };
                let default = if self.check_exact(&TokenType::Equal) {
                    self.advance();
                    Some(self.parse_expression(0)?)
                } else {
                    None
                };
                params.push((pname, ptype, default));
                if !self.match_any(&[TokenType::Comma]) {
                    break;
                }
            }
        }
        Ok(params)
    }

    fn parse_statement(&mut self) -> Result<Stmt, String> {
        self.skip_newlines_and_semicolons();
        let pub_flag = if self.check_exact(&TokenType::Pub) {
            self.advance();
            true
        } else {
            false
        };
        match &self.peek().token_type {
            TokenType::Cell => self.parse_cell(pub_flag),
            TokenType::Tilde | TokenType::Async => self.parse_fn_decl(pub_flag),
            TokenType::Macro => self.parse_macro(),
            TokenType::When => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'when'", self.peek().line)); }
                self.parse_when()
            }
            TokenType::While => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'while'", self.peek().line)); }
                self.parse_while()
            }
            TokenType::Each => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'over'", self.peek().line)); }
                self.parse_each()
            }
            TokenType::Break => {
                self.advance();
                Ok(Stmt::Break { span: Some(SourceSpan { line: self.previous().line, col: self.previous().col }) })
            }
            TokenType::Continue => {
                self.advance();
                Ok(Stmt::Continue { span: Some(SourceSpan { line: self.previous().line, col: self.previous().col }) })
            }
            TokenType::Shape => self.parse_shape(pub_flag),
            TokenType::Style => self.parse_style(pub_flag),
            TokenType::Class => self.parse_class(pub_flag),
            TokenType::Do => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'do'", self.peek().line)); }
                self.parse_do_while()
            }
            TokenType::Yield => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'yield'", self.peek().line)); }
                self.parse_yield()
            }
            TokenType::Throw => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'throw'", self.peek().line)); }
                self.parse_throw()
            }
            TokenType::Pick => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'pick'", self.peek().line)); }
                self.parse_pick()
            }
            TokenType::Dare => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'dare'", self.peek().line)); }
                self.parse_dare()
            }
            TokenType::For => {
                return Err(format!("Line {}: unexpected 'for' outside impl block", self.peek().line));
            }
            TokenType::Trait => self.parse_trait(pub_flag),
            TokenType::Impl => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'impl'", self.peek().line)); }
                self.parse_impl()
            }
            TokenType::Emit => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'emit'", self.peek().line)); }
                self.parse_emit()
            }
            TokenType::Say => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'say'", self.peek().line)); }
                self.parse_say(false)
            }
            TokenType::Shout => {
                if pub_flag { return Err(format!("Line {}: 'pub' not allowed before 'shout'", self.peek().line)); }
                self.parse_say(true)
            }
            TokenType::Load => self.parse_load(pub_flag),
            _ => self.parse_expr_stmt(),
        }
    }

    fn expect_newline_indent(&mut self, msg: &str) -> Result<(), String> {
        self.expect(&TokenType::Newline, msg)?;
        self.expect(&TokenType::Indent, msg)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
            self.skip_newlines_and_semicolons();
            if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                break;
            }
            match self.parse_statement() {
                Ok(s) => stmts.push(s),
                Err(e) => {
                    self.errors.push(e);
                    self.sync_to_statement_boundary();
                }
            }
        }
        if self.check_exact(&TokenType::Dedent) {
            self.advance();
        }
        Ok(stmts)
    }

    fn parse_cell(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });

        // Check for destructuring patterns
        if self.check_exact(&TokenType::LParen) {
            return self.parse_list_destructure(span, pub_flag);
        }
        if self.check_exact(&TokenType::LBrace) {
            return self.parse_struct_destructure(span, pub_flag);
        }

        // Regular cell name = expr
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected identifier, got {}", tok.line, t)),
        };
        let type_ann = self.parse_type();
        self.expect(&TokenType::Equal, "Expected '='")?;
        let value = self.parse_expression(0)?;
        Ok(Stmt::Let {
            span,
            pub_flag,
            name,
            type_ann,
            value: Box::new(value),
        })
    }

    fn parse_list_destructure(&mut self, span: HasSpan, pub_flag: bool) -> Result<Stmt, String> {
        self.advance(); // consume (
        let mut items = Vec::new();
        loop {
            if self.check_exact(&TokenType::RParen) {
                break;
            }
            if self.check_exact(&TokenType::DotDotDot) {
                self.advance(); // consume ...
                let tok = self.advance();
                let name = match &tok.token_type {
                    TokenType::Identifier(n) => n.clone(),
                    t => return Err(format!("Line {}: expected identifier after ..., got {}", tok.line, t)),
                };
                items.push(DestructureItem::Rest(name));
            } else {
                let tok = self.advance();
                let name = match &tok.token_type {
                    TokenType::Identifier(n) => n.clone(),
                    t => return Err(format!("Line {}: expected identifier, got {}", tok.line, t)),
                };
                items.push(DestructureItem::Name(name));
            }
            if !self.match_any(&[TokenType::Comma]) {
                break;
            }
        }
        self.expect(&TokenType::RParen, "Expected ')'")?;
        self.expect(&TokenType::Equal, "Expected '='")?;
        let value = self.parse_expression(0)?;
        Ok(Stmt::Destructure {
            span,
            pub_flag,
            target: DestructureTarget::List(items),
            value: Box::new(value),
        })
    }

    fn parse_struct_destructure(&mut self, span: HasSpan, pub_flag: bool) -> Result<Stmt, String> {
        self.advance(); // consume {
        let mut fields = Vec::new();
        loop {
            if self.check_exact(&TokenType::RBrace) {
                break;
            }
            let tok = self.advance();
            let name = match &tok.token_type {
                TokenType::Identifier(n) => n.clone(),
                t => return Err(format!("Line {}: expected field name, got {}", tok.line, t)),
            };
            fields.push(name);
            if !self.match_any(&[TokenType::Comma]) {
                break;
            }
        }
        self.expect(&TokenType::RBrace, "Expected '}'")?;
        self.expect(&TokenType::Equal, "Expected '='")?;
        let value = self.parse_expression(0)?;
        Ok(Stmt::Destructure {
            span,
            pub_flag,
            target: DestructureTarget::Struct(fields),
            value: Box::new(value),
        })
    }

    fn parse_fn_decl(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        let is_async = if self.check_exact(&TokenType::Async) {
            self.advance();
            true
        } else {
            false
        };
        self.expect(&TokenType::Tilde, "Expected '~'")?; // consume '~'
        let tok = self.advance();
        let span = Some(SourceSpan { line: tok.line, col: tok.col });
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => {
                return Err(format!(
                    "Line {}: expected function name, got {}",
                    tok.line, t
                ))
            }
        };
        let generic_params = if self.check_exact(&TokenType::Less) {
            self.parse_generic_params()?
        } else {
            Vec::new()
        };
        self.expect(&TokenType::LParen, "Expected '('")?;
        let params = self.parse_params()?;
        self.expect(&TokenType::RParen, "Expected ')'")?;
        let return_type = self.parse_return_type();
        self.expect_newline_indent("Expected indented function body")?;
        let body = self.parse_block()?;
        Ok(Stmt::Fn {
            span,
            pub_flag,
            name,
            generic_params,
            params,
            return_type,
            body,
            is_async,
        })
    }

    fn parse_macro(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected macro name, got {}", tok.line, t)),
        };
        let mut params = Vec::new();
        if self.check_exact(&TokenType::LParen) {
            self.advance();
            if !self.check_exact(&TokenType::RParen) {
                loop {
                    if self.check_exact(&TokenType::RParen) { break; }
                    let ptok = self.advance();
                    match &ptok.token_type {
                        TokenType::Identifier(n) => params.push(n.clone()),
                        t => return Err(format!("Line {}: expected macro parameter name, got {}", ptok.line, t)),
                    }
                    if !self.match_any(&[TokenType::Comma]) { break; }
                }
            }
            self.expect(&TokenType::RParen, "Expected ')'")?;
        }
        self.expect_newline_indent("Expected indented macro body")?;
        let body = self.parse_block()?;
        Ok(Stmt::Macro { span, name, params, body })
    }

    fn parse_generic_params(&mut self) -> Result<Vec<String>, String> {
        self.advance(); // consume '<'
        let mut params = Vec::new();
        loop {
            let tok = self.advance();
            match &tok.token_type {
                TokenType::Identifier(n) => params.push(n.clone()),
                t => {
                    return Err(format!(
                        "Line {}: expected generic parameter name, got {}",
                        tok.line, t
                    ))
                }
            }
            if self.check_exact(&TokenType::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.expect(&TokenType::Greater, "Expected '>'")?;
        Ok(params)
    }

    fn parse_when(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let condition = self.parse_expression(0)?;
        self.expect_newline_indent("Expected indented block after when condition")?;
        let then_branch = self.parse_block()?;
        let else_branch = if self.check_exact(&TokenType::Else) {
            self.advance();
            if self.check_exact(&TokenType::When) {
                Some(vec![self.parse_when()?])
            } else {
                self.expect_newline_indent("Expected indented block after else")?;
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If {
            span,
            condition: Box::new(condition),
            then_branch,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let condition = self.parse_expression(0)?;
        self.expect_newline_indent("Expected indented block after while condition")?;
        let body = self.parse_block()?;
        Ok(Stmt::While {
            span,
            condition: Box::new(condition),
            body,
        })
    }

    fn parse_do_while(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        self.expect_newline_indent("Expected indented block after do")?;
        let body = self.parse_block()?;
        self.expect_while_after_do()?;
        let condition = self.parse_expression(0)?;
        Ok(Stmt::DoWhile {
            span,
            condition: Box::new(condition),
            body,
        })
    }

    fn parse_yield(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let value = self.parse_expression(0)?;
        Ok(Stmt::Yield { span, value: Box::new(value) })
    }

    fn parse_throw(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let value = self.parse_expression(0)?;
        Ok(Stmt::Throw { span, value: Box::new(value) })
    }

    fn expect_while_after_do(&mut self) -> Result<(), String> {
        match self.peek().token_type {
            TokenType::While => {
                self.advance();
                Ok(())
            }
            _ => {
                let tok = self.peek().clone();
                Err(format!("Line {}: expected 'while' after do block, got '{}'", tok.line, tok.token_type))
            }
        }
    }

    fn parse_each(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let var = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected variable, got {}", tok.line, t)),
        };
        self.expect(&TokenType::In, "Expected 'in'")?;
        let iterable = self.parse_expression(0)?;
        self.expect_newline_indent("Expected indented block after over")?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            span,
            var,
            iterable: Box::new(iterable),
            body,
        })
    }

    fn parse_pick(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let value = self.parse_expression(0)?;
        self.expect_newline_indent("Expected indented pick arms")?;
        let mut arms = Vec::new();
        while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
            self.skip_newlines_and_semicolons();
            if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                break;
            }
            arms.push(self.parse_match_arm()?);
        }
        if self.check_exact(&TokenType::Dedent) {
            self.advance();
        }
        Ok(Stmt::Match {
            span,
            value: Box::new(value),
            arms,
        })
    }

    fn parse_shape(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => {
                return Err(format!(
                    "Line {}: expected shape name, got {}",
                    tok.line, t
                ))
            }
        };
        let fields = self.parse_struct_fields()?;
        Ok(Stmt::Struct { span, pub_flag, name, fields })
    }

    fn parse_struct_fields(&mut self) -> Result<Vec<(String, Option<Type>)>, String> {
        let mut fields = Vec::new();
        if self.check_exact(&TokenType::Newline) {
            self.advance();
            if self.check_exact(&TokenType::Indent) {
                self.advance();
                while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
                    self.skip_newlines_and_semicolons();
                    if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                        break;
                    }
                    let tok = self.advance();
                    let fname = match &tok.token_type {
                        TokenType::Identifier(n) => n.clone(),
                        t => {
                            return Err(format!(
                                "Line {}: expected field name, got {}",
                                tok.line, t
                            ))
                        }
                    };
                    let ftype = self.parse_type();
                    fields.push((fname, ftype));
                    self.skip_newlines_and_semicolons();
                }
                if self.check_exact(&TokenType::Dedent) {
                    self.advance();
                }
            }
        }
        Ok(fields)
    }

    fn parse_style(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected style name, got {}", tok.line, t)),
        };
        let mut variants = Vec::new();
        if self.check_exact(&TokenType::Newline) {
            self.advance();
            if self.check_exact(&TokenType::Indent) {
                self.advance();
                while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
                    self.skip_newlines_and_semicolons();
                    if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                        break;
                    }
                    let tok = self.advance();
                    let vname = match &tok.token_type {
                        TokenType::Identifier(n) => n.clone(),
                        t => {
                            return Err(format!(
                                "Line {}: expected variant name, got {}",
                                tok.line, t
                            ))
                        }
                    };
                    let fields = if self.check_exact(&TokenType::Newline) {
                        // Check if there is an indented block for variant fields
                        self.advance();
                        if self.check_exact(&TokenType::Indent) {
                            self.advance();
                            let mut vfields = Vec::new();
                            while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
                                self.skip_newlines_and_semicolons();
                                if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                                    break;
                                }
                                let ftok = self.advance();
                                let fname = match &ftok.token_type {
                                    TokenType::Identifier(n) => n.clone(),
                                    t => {
                                        return Err(format!(
                                            "Line {}: expected field name, got {}",
                                            ftok.line, t
                                        ))
                                    }
                                };
                                let ftype = self.parse_type();
                                vfields.push((fname, ftype));
                                self.skip_newlines_and_semicolons();
                            }
                            if self.check_exact(&TokenType::Dedent) {
                                self.advance();
                            }
                            vfields
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };
                    variants.push(crate::ast::EnumVariant {
                        name: vname,
                        fields,
                    });
                    self.skip_newlines_and_semicolons();
                }
                if self.check_exact(&TokenType::Dedent) {
                    self.advance();
                }
            }
        }
        Ok(Stmt::Enum { span, pub_flag, name, variants })
    }

    fn parse_class(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected class name, got {}", tok.line, t)),
        };
        let mut extends = None;
        if self.check_exact(&TokenType::Extends) {
            self.advance();
            let ptok = self.advance();
            extends = Some(match &ptok.token_type {
                TokenType::Identifier(n) => n.clone(),
                t => return Err(format!("Line {}: expected parent class name, got {}", ptok.line, t)),
            });
        }
        let mut methods = Vec::new();
        if self.check_exact(&TokenType::Newline) {
            self.advance();
            if self.check_exact(&TokenType::Indent) {
                self.advance();
                while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
                    self.skip_newlines_and_semicolons();
                    if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                        break;
                    }
                    self.expect(&TokenType::Tilde, "Expected '~' for method")?;
                    let mtok = self.advance();
                    let mname = match &mtok.token_type {
                        TokenType::Identifier(n) => n.clone(),
                        t => return Err(format!("Line {}: expected method name, got {}", mtok.line, t)),
                    };
                    self.expect(&TokenType::LParen, "Expected '('")?;
                    let params = self.parse_params()?;
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    // skip return type (methods don't support it yet)
                    let _return_type = self.parse_return_type();
                    self.expect_newline_indent("Expected indented method body")?;
                    let body = self.parse_block()?;
                    methods.push(crate::ast::ClassMethod { name: mname, params, body });
                    self.skip_newlines_and_semicolons();
                }
                if self.check_exact(&TokenType::Dedent) {
                    self.advance();
                }
            }
        }
        Ok(Stmt::Class { span, pub_flag, name, extends, methods })
    }

    fn parse_trait_method(&mut self) -> Result<TraitMethod, String> {
        self.advance(); // consume ~ or fn
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected method name, got {}", tok.line, t)),
        };
        self.expect(&TokenType::LParen, "Expected '('")?;
        let params = self.parse_params()?;
        self.expect(&TokenType::RParen, "Expected ')'")?;
        let return_type = self.parse_return_type();
        let body = if self.check_exact(&TokenType::Newline) {
            self.advance();
            if self.check_exact(&TokenType::Indent) {
                self.advance();
                let b = self.parse_block()?;
                if self.check_exact(&TokenType::Dedent) {
                    self.advance();
                }
                Some(b)
            } else {
                None
            }
        } else {
            None
        };
        Ok(TraitMethod { name, params, return_type, body })
    }

    fn parse_trait(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected trait name, got {}", tok.line, t)),
        };
        let mut methods = Vec::new();
        if self.check_exact(&TokenType::Newline) {
            self.advance();
            if self.check_exact(&TokenType::Indent) {
                self.advance();
                while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
                    self.skip_newlines_and_semicolons();
                    if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                        break;
                    }
                    methods.push(self.parse_trait_method()?);
                    self.skip_newlines_and_semicolons();
                }
                if self.check_exact(&TokenType::Dedent) {
                    self.advance();
                }
            }
        }
        Ok(Stmt::Trait { span, pub_flag, name, methods })
    }

    fn parse_impl(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let trait_name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected trait name, got {}", tok.line, t)),
        };
        self.expect(&TokenType::For, "Expected 'for' in impl")?;
        let tok = self.advance();
        let type_name = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => return Err(format!("Line {}: expected type name, got {}", tok.line, t)),
        };
        let mut methods = Vec::new();
        if self.check_exact(&TokenType::Newline) {
            self.advance();
            if self.check_exact(&TokenType::Indent) {
                self.advance();
                while !self.check_exact(&TokenType::Dedent) && !self.is_at_end() {
                    self.skip_newlines_and_semicolons();
                    if self.check_exact(&TokenType::Dedent) || self.is_at_end() {
                        break;
                    }
                    self.expect(&TokenType::Tilde, "Expected '~' for impl method")?;
                    let mtok = self.advance();
                    let mname = match &mtok.token_type {
                        TokenType::Identifier(n) => n.clone(),
                        t => return Err(format!("Line {}: expected method name, got {}", mtok.line, t)),
                    };
                    self.expect(&TokenType::LParen, "Expected '('")?;
                    let params = self.parse_params()?;
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    let return_type = self.parse_return_type();
                    self.expect_newline_indent("Expected indented method body")?;
                    let body = self.parse_block()?;
                    methods.push(TraitMethodImpl { name: mname, params, return_type, body });
                    self.skip_newlines_and_semicolons();
                }
                if self.check_exact(&TokenType::Dedent) {
                    self.advance();
                }
            }
        }
        Ok(Stmt::Impl { span, trait_name, type_name, methods })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        let pattern = self.parse_match_pattern()?;
        let guard = if self.check_exact(&TokenType::When) {
            self.advance();
            Some(self.parse_expression(0)?)
        } else {
            None
        };
        if self.check_exact(&TokenType::Arrow) || self.check_exact(&TokenType::FatArrow) {
            self.advance();
        }
        let body = if self.check_exact(&TokenType::Newline) {
            self.advance();
            self.expect(
                &TokenType::Indent,
                "Expected indented block after match arm",
            )?;
            self.parse_block()?
        } else {
            let expr = self.parse_expression(0)?;
            vec![Stmt::Expr {
                span: None,
                expr: Box::new(expr),
            }]
        };
        Ok(MatchArm {
            pattern,
            guard,
            body,
        })
    }

    fn parse_match_pattern(&mut self) -> Result<MatchPattern, String> {
        // Handle or-patterns: pattern1 | pattern2 | ...
        let mut patterns = Vec::new();
        patterns.push(self.parse_single_match_pattern()?);
        while self.check_exact(&TokenType::Pipe) {
            self.advance();
            patterns.push(self.parse_single_match_pattern()?);
        }
        if patterns.len() == 1 {
            Ok(patterns.into_iter().next().unwrap())
        } else {
            Ok(MatchPattern::Or(patterns))
        }
    }

    fn parse_single_match_pattern(&mut self) -> Result<MatchPattern, String> {
        if self.check_exact(&TokenType::Bang) {
            self.advance();
            self.expect(&TokenType::Equal, "Expected '=' after !")?;
            let expr = self.parse_expression(0)?;
            Ok(MatchPattern::Literal(Expr::UnaryOp {
                op: UnaryOpKind::Not,
                right: Box::new(expr),
            }))
        } else if let TokenType::Identifier(name) = &self.peek().token_type {
            let name = name.clone();
            self.advance();
            if name == "_" {
                return Ok(MatchPattern::Wildcard);
            }
            // Could be a destructure pattern or a literal identifier
            if self.check_exact(&TokenType::LParen) {
                // Destructure: Foo(a, b, c)
                self.advance();
                let mut fields = Vec::new();
                if !self.check_exact(&TokenType::RParen) {
                    loop {
                        if self.check_exact(&TokenType::RParen) {
                            break;
                        }
                        let tok = self.advance();
                        let fname = match &tok.token_type {
                            TokenType::Identifier(n) => n.clone(),
                            t => {
                                return Err(format!(
                                    "Line {}: expected field name or '_', got {}",
                                    tok.line, t
                                ))
                            }
                        };
                        fields.push(fname);
                        if !self.match_any(&[TokenType::Comma]) {
                            break;
                        }
                    }
                }
                self.expect(&TokenType::RParen, "Expected ')'")?;
                Ok(MatchPattern::Destructure(name, fields))
            } else {
                Ok(MatchPattern::Binding(name))
            }
        } else {
            let expr = self.parse_expression(0)?;
            Ok(MatchPattern::Literal(expr))
        }
    }

    fn parse_dare(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        self.expect_newline_indent("Expected indented block after dare")?;
        let body = self.parse_block()?;
        self.expect(&TokenType::Catch, "Expected 'catch' after dare")?;
        let tok = self.advance();
        let catch_var = match &tok.token_type {
            TokenType::Identifier(n) => n.clone(),
            t => {
                return Err(format!(
                    "Line {}: expected variable name after catch, got {}",
                    tok.line, t
                ))
            }
        };
        self.expect_newline_indent("Expected indented block after catch")?;
        let catch_body = self.parse_block()?;
        Ok(Stmt::Try {
            span,
            body,
            catch_var,
            catch_body,
        })
    }

    fn parse_emit(&mut self) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let value = if self.check_exact(&TokenType::Newline)
            || self.check_exact(&TokenType::Semicolon)
            || self.check_exact(&TokenType::Dedent)
            || self.check_exact(&TokenType::Eof)
        {
            Expr::Nil
        } else {
            self.parse_expression(0)?
        };
        Ok(Stmt::Return {
            span,
            value: Box::new(value),
        })
    }

    fn parse_say(&mut self, newline: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let v = self.parse_expression(0)?;
        Ok(Stmt::Print {
            span,
            value: Box::new(v),
            newline,
        })
    }

    fn parse_load(&mut self, pub_flag: bool) -> Result<Stmt, String> {
        self.advance();
        let span = Some(SourceSpan { line: self.previous().line, col: self.previous().col });
        let tok = self.advance();
        let path = match &tok.token_type {
            TokenType::String(s) => s.replace('.', "/"),
            TokenType::Identifier(part) => {
                let mut p = part.clone();
                loop {
                    if self.match_any(&[TokenType::Slash, TokenType::Dot]) {
                        let next = self.advance();
                        match &next.token_type {
                            TokenType::Identifier(id) => {
                                p.push('/');
                                p.push_str(id);
                            }
                            t => {
                                return Err(format!(
                                    "Line {}: expected identifier after '/', got {}",
                                    next.line, t
                                ))
                            }
                        }
                    } else {
                        break;
                    }
                }
                p
            }
            t => {
                return Err(format!(
                    "Line {}: expected string path or identifier, got {}",
                    tok.line, t
                ))
            }
        };
        let alias = if self.check_exact(&TokenType::As) {
            self.advance();
            let tok = self.advance();
            match &tok.token_type {
                TokenType::Identifier(n) => Some(n.clone()),
                t => {
                    return Err(format!(
                        "Line {}: expected alias name after 'as', got {}",
                        tok.line, t
                    ))
                }
            }
        } else {
            None
        };
        Ok(Stmt::Import { span, pub_flag, path, alias })
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, String> {
        let span = Some(SourceSpan { line: self.peek().line, col: self.peek().col });
        let expr = self.parse_expression(0)?;
        Ok(Stmt::Expr {
            span,
            expr: Box::new(expr),
        })
    }

    fn parse_comp_clauses(&mut self) -> Result<Vec<crate::ast::CompClause>, String> {
        let mut clauses = Vec::new();
        loop {
            if !self.check_exact(&TokenType::For) {
                break;
            }
            self.advance(); // consume 'for'
            let tok = self.advance();
            let var = match &tok.token_type {
                TokenType::Identifier(n) => n.clone(),
                t => return Err(format!("Line {}: expected variable name after 'for', got {}", tok.line, t)),
            };
            self.expect(&TokenType::In, "Expected 'in' after for variable")?;
            let iterable = self.parse_expression(0)?;
            let mut conditions = Vec::new();
            while self.check_exact(&TokenType::When) {
                self.advance();
                conditions.push(self.parse_expression(0)?);
            }
            clauses.push(crate::ast::CompClause {
                var,
                iterable: Box::new(iterable),
                conditions,
            });
        }
        Ok(clauses)
    }

    fn parse_expression(&mut self, min_prec: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_prefix()?;
        loop {
            let prec = self.get_precedence();
            if prec < min_prec || prec == 0 {
                if self.check_exact(&TokenType::PipeArrow) {
                    lhs = self.parse_pipe(lhs)?;
                    continue;
                }
                break;
            }
            lhs = self.parse_infix(lhs, prec)?;
        }
        Ok(lhs)
    }

    fn get_precedence(&self) -> u8 {
        match &self.peek().token_type {
            TokenType::Equal
            | TokenType::PlusEqual
            | TokenType::MinusEqual
            | TokenType::StarEqual
            | TokenType::SlashEqual
            | TokenType::PercentEqual => 1,
            TokenType::Question => 1,
            TokenType::OrOr => 2,
            TokenType::AndAnd => 3,
            TokenType::EqualEqual | TokenType::BangEqual | TokenType::In => 4,
            TokenType::Less
            | TokenType::LessEqual
            | TokenType::Greater
            | TokenType::GreaterEqual => 5,
            TokenType::Pipe => 6,
            TokenType::Caret => 7,
            TokenType::Ampersand => 8,
            TokenType::ShiftLeft | TokenType::ShiftRight => 9,
            TokenType::Plus | TokenType::Minus => 10,
            TokenType::Star | TokenType::Slash | TokenType::Percent => 11,
            TokenType::DotDot => 12,
            TokenType::LBracket => 13,
            TokenType::Dot => 14,
            _ => 0,
        }
    }

    fn parse_prefix(&mut self) -> Result<Expr, String> {
        let token = self.advance().clone();
        match &token.token_type {
            TokenType::DotDotDot => {
                let expr = self.parse_expression(12)?;   // high enough to consume most of expression
                Ok(Expr::Spread(Box::new(expr)))
            }
            TokenType::Int(n) => Ok(Expr::Int(*n)),
            TokenType::Float(n) => Ok(Expr::Float(*n)),
            TokenType::String(s) => Ok(Expr::String(s.clone())),
            TokenType::StringPart(s) => {
                let mut parts = Vec::new();
                parts.push(Expr::String(s.clone()));
                loop {
                    let tok = self.peek().clone();
                    match &tok.token_type {
                        TokenType::ExprString(raw) => {
                            self.advance();
                            let expr_tokens = crate::lexer::Lexer::new(raw).tokenize();
                            let mut expr_parser = Parser::new(expr_tokens);
                            let expr = expr_parser
                                .parse_expression(0)
                                .map_err(|e| format!("In string interpolation: {}", e))?;
                            parts.push(expr);
                        }
                        TokenType::StringPart(text) => {
                            self.advance();
                            parts.push(Expr::String(text.clone()));
                        }
                        TokenType::StringEnd => {
                            self.advance();
                            break;
                        }
                        _ => {
                            return Err(format!(
                                "Line {}: unexpected token '{}' in string interpolation",
                                tok.line, tok.token_type
                            ))
                        }
                    }
                }
                Ok(Expr::StringInterp(parts))
            }
            TokenType::Boolean(b) => Ok(Expr::Boolean(*b)),
            TokenType::None => Ok(Expr::Nil),
            TokenType::Super => Ok(Expr::Super),
            TokenType::Backslash => {
                let generic_params = if self.check_exact(&TokenType::Less) {
                    self.parse_generic_params()?
                } else {
                    Vec::new()
                };
                self.expect(&TokenType::LParen, "Expected '(' after \\")?;
                let params = self.parse_params()?;
                self.expect(&TokenType::RParen, "Expected ')'")?;
                let _return_type = self.parse_return_type();
                if self.check_exact(&TokenType::Newline) {
                    self.advance();
                    self.expect(&TokenType::Indent, "Expected indented block in lambda")?;
                    let body = self.parse_block()?;
                    Ok(Expr::Fn {
                        generic_params,
                        params,
                        body,
                    })
                } else {
                    let expr = self.parse_expression(0)?;
                    Ok(Expr::Fn {
                        generic_params,
                        params,
                        body: vec![Stmt::Expr {
                            span: None,
                            expr: Box::new(expr),
                        }],
                    })
                }
            }
            TokenType::LBracket => {
                let mut elems = Vec::new();
                if !self.check_exact(&TokenType::RBracket) {
                    let first = self.parse_expression(0)?;
                    if self.check_exact(&TokenType::For) {
                        // List comprehension: [expr for x in iter when cond]
                        let clauses = self.parse_comp_clauses()?;
                        self.expect(&TokenType::RBracket, "Expected ']'")?;
                        return Ok(Expr::ListComp { expr: Box::new(first), clauses });
                    }
                    elems.push(first);
                    while self.match_any(&[TokenType::Comma]) {
                        if self.check_exact(&TokenType::RBracket) {
                            break;
                        }
                        elems.push(self.parse_expression(0)?);
                    }
                }
                self.expect(&TokenType::RBracket, "Expected ']'")?;
                Ok(Expr::List(elems))
            }
            TokenType::LBrace => {
                if self.check_exact(&TokenType::RBrace) {
                    self.advance();
                    return Ok(Expr::Dict(Vec::new()));
                }
                let first = self.parse_expression(0)?;
                if self.check_exact(&TokenType::Colon) {
                    self.advance();
                    let val = self.parse_expression(0)?;
                    if self.check_exact(&TokenType::For) {
                        // Dict comprehension: {key: value for x in iter when cond}
                        let clauses = self.parse_comp_clauses()?;
                        self.expect(&TokenType::RBrace, "Expected '}}'")?;
                        return Ok(Expr::DictComp { key: Box::new(first), value: Box::new(val), clauses });
                    }
                    let mut pairs = vec![(first, val)];
                    while self.match_any(&[TokenType::Comma]) {
                        if self.check_exact(&TokenType::RBrace) {
                            break;
                        }
                        let k = self.parse_expression(0)?;
                        self.expect(&TokenType::Colon, "Expected ':'")?;
                        let v = self.parse_expression(0)?;
                        pairs.push((k, v));
                    }
                    self.expect(&TokenType::RBrace, "Expected '}}'")?;
                    Ok(Expr::Dict(pairs))
                } else {
                    if self.check_exact(&TokenType::For) {
                        // Set comprehension: {expr for x in iter when cond}
                        let clauses = self.parse_comp_clauses()?;
                        self.expect(&TokenType::RBrace, "Expected '}}'")?;
                        return Ok(Expr::SetComp { expr: Box::new(first), clauses });
                    }
                    let mut elems = vec![first];
                    while self.match_any(&[TokenType::Comma]) {
                        if self.check_exact(&TokenType::RBrace) {
                            break;
                        }
                        elems.push(self.parse_expression(0)?);
                    }
                    self.expect(&TokenType::RBrace, "Expected '}}'")?;
                    Ok(Expr::Set(elems))
                }
            }
            TokenType::Identifier(name) => {
                let name = name.clone();
                if self.check_exact(&TokenType::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check_exact(&TokenType::RParen) {
                        loop {
                            if self.check_exact(&TokenType::RParen) {
                                break;
                            }
                            args.push(self.parse_expression(0)?);
                            if !self.match_any(&[TokenType::Comma]) {
                                break;
                            }
                        }
                    }
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    Ok(Expr::Call { callee: name, args })
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            TokenType::TypeInt
            | TokenType::TypeFloat
            | TokenType::TypeString
            | TokenType::TypeBool
            | TokenType::TypeList
            | TokenType::TypeDict => {
                let name = token_type_to_ident(&token.token_type);
                if self.check_exact(&TokenType::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check_exact(&TokenType::RParen) {
                        loop {
                            if self.check_exact(&TokenType::RParen) {
                                break;
                            }
                            args.push(self.parse_expression(0)?);
                            if !self.match_any(&[TokenType::Comma]) {
                                break;
                            }
                        }
                    }
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    Ok(Expr::Call { callee: name, args })
                } else {
                    Ok(Expr::Variable(name))
                }
            }
            TokenType::Minus => {
                let expr = self.parse_expression(12)?;
                Ok(Expr::UnaryOp {
                    op: UnaryOpKind::Negate,
                    right: Box::new(expr),
                })
            }
            TokenType::Tilde => {
                let expr = self.parse_expression(12)?;
                Ok(Expr::UnaryOp {
                    op: UnaryOpKind::BitNot,
                    right: Box::new(expr),
                })
            }
            TokenType::Bang => {
                let expr = self.parse_expression(12)?;
                Ok(Expr::UnaryOp {
                    op: UnaryOpKind::Not,
                    right: Box::new(expr),
                })
            }
            TokenType::Await => {
                let expr = self.parse_expression(0)?;
                Ok(Expr::Await { value: Box::new(expr) })
            }
            TokenType::LParen => {
                if self.check_exact(&TokenType::RParen) {
                    self.advance();
                    return Ok(Expr::Tuple(Vec::new()));
                }
                let first = self.parse_expression(0)?;
                if self.check_exact(&TokenType::Comma) {
                    // Tuple: (a, b, c) or (a,)
                    let mut items = vec![first];
                    while self.match_any(&[TokenType::Comma]) {
                        if self.check_exact(&TokenType::RParen) {
                            break;
                        }
                        items.push(self.parse_expression(0)?);
                    }
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    Ok(Expr::Tuple(items))
                } else {
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    Ok(Expr::Grouping(Box::new(first)))
                }
            }
            t => Err(format!("Line {}: unexpected token '{}'", token.line, t)),
        }
    }

    fn parse_infix(&mut self, lhs: Expr, min_prec: u8) -> Result<Expr, String> {
        let token = self.peek().clone();
        match &token.token_type {
            TokenType::Plus
            | TokenType::Minus
            | TokenType::Star
            | TokenType::Slash
            | TokenType::Percent
            | TokenType::EqualEqual
            | TokenType::BangEqual
            | TokenType::Less
            | TokenType::LessEqual
            | TokenType::Greater
            | TokenType::GreaterEqual
            | TokenType::AndAnd
            | TokenType::OrOr
            | TokenType::In
            | TokenType::Pipe
            | TokenType::Ampersand
            | TokenType::Caret
            | TokenType::ShiftLeft
            | TokenType::ShiftRight => {
                self.advance();
                let op = token_type_to_binop(&token.token_type);
                let rhs = self.parse_expression(min_prec)?;
                Ok(Expr::BinaryOp {
                    left: Box::new(lhs),
                    op,
                    right: Box::new(rhs),
                })
            }
            TokenType::Question => {
                self.advance();
                // Check if this is postfix try operator (expr?) vs ternary (a ? b : c)
                if self.check_exact(&TokenType::Newline)
                    || self.check_exact(&TokenType::Semicolon)
                    || self.check_exact(&TokenType::Dedent)
                    || self.check_exact(&TokenType::RParen)
                    || self.check_exact(&TokenType::RBracket)
                    || self.check_exact(&TokenType::Eof)
                    || self.check_exact(&TokenType::Comma)
                {
                    return Ok(Expr::Try { expr: Box::new(lhs) });
                }
                // Ternary: a ? b : c
                let then_expr = self.parse_expression(min_prec)?;
                self.expect(&TokenType::Colon, "Expected ':' in ternary")?;
                let else_expr = self.parse_expression(min_prec)?;
                Ok(Expr::Ternary {
                    condition: Box::new(lhs),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                })
            }
            TokenType::PlusEqual
            | TokenType::MinusEqual
            | TokenType::StarEqual
            | TokenType::SlashEqual
            | TokenType::PercentEqual => {
                self.advance();
                let op = token_type_to_compound_op(&token.token_type);
                let name = match &lhs {
                    Expr::Variable(n) => n.clone(),
                    _ => return Err(format!("Line {}: invalid assignment target", token.line)),
                };
                let rhs = self.parse_expression(1)?;
                Ok(Expr::CompoundAssign {
                    name,
                    op,
                    value: Box::new(rhs),
                })
            }
            TokenType::Equal => {
                self.advance();
                let value = self.parse_expression(1)?;
                match lhs {
                    Expr::Variable(name) => Ok(Expr::Assignment {
                        name,
                        value: Box::new(value),
                    }),
                    Expr::FieldAccess { object, field } => Ok(Expr::FieldAssign {
                        object,
                        field,
                        value: Box::new(value),
                    }),
                    _ => Err(format!("Line {}: invalid assignment target", token.line)),
                }
            }
            TokenType::LBracket => {
                self.advance();
                // Check for colon-first slicing: list[:end] or list[:]
                if self.check_exact(&TokenType::Colon) {
                    self.advance();
                    let end = if self.check_exact(&TokenType::RBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expression(0)?))
                    };
                    self.expect(&TokenType::RBracket, "Expected ']'")?;
                    return Ok(Expr::Slice {
                        object: Box::new(lhs),
                        start: None,
                        end,
                    });
                }
                let first = self.parse_expression(0)?;
                if self.check_exact(&TokenType::Colon) {
                    self.advance();
                    if self.check_exact(&TokenType::RBracket) {
                        self.advance();
                        Ok(Expr::Slice {
                            object: Box::new(lhs),
                            start: Some(Box::new(first)),
                            end: None,
                        })
                    } else {
                        let end = self.parse_expression(0)?;
                        self.expect(&TokenType::RBracket, "Expected ']'")?;
                        Ok(Expr::Slice {
                            object: Box::new(lhs),
                            start: Some(Box::new(first)),
                            end: Some(Box::new(end)),
                        })
                    }
                } else {
                    self.expect(&TokenType::RBracket, "Expected ']'")?;
                    Ok(Expr::Index {
                        object: Box::new(lhs),
                        index: Box::new(first),
                    })
                }
            }
            TokenType::DotDot => {
                self.advance();
                let rhs = self.parse_expression(min_prec)?;
                Ok(Expr::Range {
                    start: Box::new(lhs),
                    end: Box::new(rhs),
                })
            }
            TokenType::Dot => {
                self.advance();
                let tok = self.advance();
                let field = match &tok.token_type {
                    TokenType::Identifier(n) => n.clone(),
                    t => {
                        return Err(format!(
                            "Line {}: expected field/method name, got {}",
                            tok.line, t
                        ))
                    }
                };
                if self.check_exact(&TokenType::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    if !self.check_exact(&TokenType::RParen) {
                        loop {
                            if self.check_exact(&TokenType::RParen) {
                                break;
                            }
                            args.push(self.parse_expression(0)?);
                            if !self.match_any(&[TokenType::Comma]) {
                                break;
                            }
                        }
                    }
                    self.expect(&TokenType::RParen, "Expected ')'")?;
                    Ok(Expr::MethodCall {
                        object: Box::new(lhs),
                        method: field,
                        args,
                    })
                } else {
                    Ok(Expr::FieldAccess {
                        object: Box::new(lhs),
                        field,
                    })
                }
            }
            _ => Ok(lhs),
        }
    }

    fn parse_pipe(&mut self, lhs: Expr) -> Result<Expr, String> {
        self.advance(); // consume |>
        let rhs = self.parse_expression(0)?;
        match rhs {
            Expr::Call { callee, mut args } => {
                args.insert(0, lhs);
                Ok(Expr::Call { callee, args })
            }
            Expr::MethodCall { object, method, mut args } => {
                args.insert(0, lhs);
                Ok(Expr::MethodCall { object, method, args })
            }
            Expr::Variable(name) => {
                Ok(Expr::Call {
                    callee: name,
                    args: vec![lhs],
                })
            }
            Expr::FieldAccess { object, field } => {
                Ok(Expr::MethodCall {
                    object,
                    method: field,
                    args: vec![lhs],
                })
            }
            other => Err(format!("|> requires a function or method call on the right, got {:?}", other)),
        }
    }
}

fn token_type_to_ident(tt: &TokenType) -> String {
    match tt {
        TokenType::TypeInt => "int".into(),
        TokenType::TypeFloat => "float".into(),
        TokenType::TypeString => "string".into(),
        TokenType::TypeBool => "bool".into(),
        TokenType::TypeList => "list".into(),
        TokenType::TypeDict => "dict".into(),
        _ => format!("{}", tt),
    }
}

fn token_type_to_binop(tt: &TokenType) -> BinaryOpKind {
    match tt {
        TokenType::Plus => BinaryOpKind::Add,
        TokenType::Minus => BinaryOpKind::Subtract,
        TokenType::Star => BinaryOpKind::Multiply,
        TokenType::Slash => BinaryOpKind::Divide,
        TokenType::Percent => BinaryOpKind::Modulo,
        TokenType::EqualEqual => BinaryOpKind::Equal,
        TokenType::BangEqual => BinaryOpKind::NotEqual,
        TokenType::Less => BinaryOpKind::Less,
        TokenType::LessEqual => BinaryOpKind::LessEqual,
        TokenType::Greater => BinaryOpKind::Greater,
        TokenType::GreaterEqual => BinaryOpKind::GreaterEqual,
        TokenType::AndAnd => BinaryOpKind::And,
        TokenType::OrOr => BinaryOpKind::Or,
        TokenType::In => BinaryOpKind::In,
        TokenType::Pipe => BinaryOpKind::BitOr,
        TokenType::Ampersand => BinaryOpKind::BitAnd,
        TokenType::Caret => BinaryOpKind::BitXor,
        TokenType::ShiftLeft => BinaryOpKind::ShiftLeft,
        TokenType::ShiftRight => BinaryOpKind::ShiftRight,
        _ => unreachable!(),
    }
}

fn token_type_to_compound_op(tt: &TokenType) -> BinaryOpKind {
    match tt {
        TokenType::PlusEqual => BinaryOpKind::Add,
        TokenType::MinusEqual => BinaryOpKind::Subtract,
        TokenType::StarEqual => BinaryOpKind::Multiply,
        TokenType::SlashEqual => BinaryOpKind::Divide,
        TokenType::PercentEqual => BinaryOpKind::Modulo,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(source: &str) -> Result<Vec<Stmt>, String> {
        let tokens = Lexer::new(source).tokenize();
        Parser::new(tokens).parse()
    }

    #[test]
    fn test_parse_cell() {
        let stmts = parse("cell x = 5").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_fn() {
        let stmts = parse("~foo(x, y)\n    emit x + y").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_when_else() {
        let stmts = parse("when yes\n    x = 1\nelse\n    x = 2").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_while() {
        let stmts = parse("while x < 10\n    x = x + 1").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_each() {
        let stmts = parse("over i in 0..10\n    shout(i)").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_pick() {
        let stmts =
            parse("pick x\n    1 ->\n        \"one\"\n    _ ->\n        \"other\"").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_dare_catch() {
        let stmts = parse("dare\n    risky()\ncatch e\n    shout(e)").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_load_as() {
        let stmts = parse(r#"load "math.eltr" as math"#).unwrap();
        assert!(matches!(
            &stmts[0],
            Stmt::Import {
                span: _,
                path: _,
                alias: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn test_parse_error_recovery() {
        let result = parse("cell x =\ncell y = 2");
        assert!(result.is_err() || !result.as_ref().is_ok_and(|s| s.is_empty()));
    }

    #[test]
    fn test_parse_expressions() {
        let stmts = parse("x + y * 3\n(a + b) / c\n-x\n!done").unwrap();
        assert_eq!(stmts.len(), 4);
    }

    #[test]
    fn test_parse_method_call() {
        let stmts = parse("obj.method(1, 2)").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_parse_compound_assign() {
        let stmts = parse("x += 1\ny %= 2").unwrap();
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn test_parse_type_annotations() {
        let stmts = parse("cell x: int = 5\ncell y: string = \"hi\"").unwrap();
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn test_parse_list_dict() {
        let stmts = parse("cell arr = [1, 2, 3]\ncell d = {\"k\": \"v\"}").unwrap();
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn test_parse_range() {
        let stmts = parse("cell r = 0..10").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_invalid_assignment() {
        let result = parse("5 = y");
        assert!(result.is_err());
    }

    #[test]
    fn test_trailing_commas() {
        let stmts = parse("cell arr = [1, 2,]\ncell d = {\"a\": 1,}\n~f(a, b,)\n    none\nf(1, 2,)")
            .unwrap();
        assert_eq!(stmts.len(), 4);
    }

    #[test]
    fn test_empty_statements() {
        let stmts = parse("cell x = 1\ncell y = 2").unwrap();
        assert_eq!(stmts.len(), 2);
    }

    #[test]
    fn test_nested_blocks() {
        let stmts = parse("when yes\n    when no\n        cell x = 1").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_else_when_chain() {
        let stmts = parse("when a\n    1\nelse when b\n    2\nelse\n    3").unwrap();
        assert_eq!(stmts.len(), 1);
    }

    #[test]
    fn test_pick_arrow() {
        let stmts =
            parse("pick x\n    1 ->\n        \"one\"\n    _ ->\n        \"other\"").unwrap();
        assert_eq!(stmts.len(), 1);
    }
}
