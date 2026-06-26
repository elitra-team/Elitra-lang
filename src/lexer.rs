use crate::token::{Token, TokenType};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    pending: Vec<Token>,
    indent_stack: Vec<usize>,
    bracket_depth: usize,
    at_line_start: bool,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 0,
            pending: Vec::new(),
            indent_stack: vec![0],
            bracket_depth: 0,
            at_line_start: true,
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            if token.token_type == TokenType::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        tokens
    }

    fn tok(&self, tt: TokenType) -> Token {
        Token::new_col(tt, self.line, self.col)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        self.col += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 0;
        }
        Some(ch)
    }

    fn skip_inline_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment_line(&mut self) {
        // Skip until but NOT past newline
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_comment_block(&mut self) -> Result<(), String> {
        loop {
            match self.advance() {
                None => return Err("Unterminated block comment".to_string()),
                Some('*') if self.peek() == Some('/') => {
                    self.advance();
                    return Ok(());
                }
                Some(_) => continue,
            }
        }
    }

    fn handle_line_start(&mut self) -> Option<Token> {
        let mut indent = 0;
        let mut i = self.pos;
        while i < self.chars.len() {
            match self.chars[i] {
                ' ' => {
                    indent += 1;
                    i += 1;
                }
                '\t' => {
                    indent = (indent / 4 + 1) * 4;
                    i += 1;
                }
                _ => break,
            }
        }

        // Check for blank / comment-only lines
        if i < self.chars.len() && self.chars[i] == '\n' {
            // blank line: skip it
            self.pos = i + 1;
            self.line += 1;
            self.at_line_start = true;
            return None;
        }
        if i + 1 < self.chars.len() && self.chars[i] == '/' && self.chars[i + 1] == '/' {
            // comment-only line: skip to \n but don't consume it
            self.pos = i; // stay at // so next iteration handles it as a // comment
            self.at_line_start = false;
            return None;
        }
        if i + 1 < self.chars.len() && self.chars[i] == '/' && self.chars[i + 1] == '*' {
            self.pos = i;
            self.at_line_start = false;
            return None;
        }

        let top = *self.indent_stack.last().unwrap_or(&0);
        if indent > top {
            self.indent_stack.push(indent);
            self.pos = i;
            self.at_line_start = false;
            Some(self.tok(TokenType::Indent))
        } else if indent < top {
            // pop one level, leave at_line_start for re-check
            self.indent_stack.pop();
            // Don't advance past indent — next call re-checks
            Some(self.tok(TokenType::Dedent))
        } else {
            self.pos = i;
            self.at_line_start = false;
            None
        }
    }

    fn queue_final_dedents(&mut self) {
        while *self.indent_stack.last().unwrap_or(&0) > 0 {
            self.indent_stack.pop();
            self.pending.push(self.tok(TokenType::Dedent));
        }
    }

    pub fn next_token(&mut self) -> Token {
        'outer: loop {
            if let Some(t) = self.pending.pop() {
                return t;
            }

            if self.at_line_start && self.bracket_depth == 0 {
                if let Some(tok) = self.handle_line_start() {
                    return tok;
                }
                if self.at_line_start {
                    continue 'outer;
                }
            }

            self.skip_inline_whitespace();

            let token_type = match self.advance() {
                None => {
                    self.queue_final_dedents();
                    if let Some(t) = self.pending.pop() {
                        return t;
                    }
                    TokenType::Eof
                }
                Some('\n') => {
                    self.at_line_start = true;
                    if self.bracket_depth > 0 {
                        continue 'outer;
                    }
                    TokenType::Newline
                }
                Some(ch) => match ch {
                    '+' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::PlusEqual
                        }
                        _ => TokenType::Plus,
                    },
                    '-' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::MinusEqual
                        }
                        Some('>') => {
                            self.advance();
                            TokenType::Arrow
                        }
                        _ => TokenType::Minus,
                    },
                    '*' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::StarEqual
                        }
                        _ => TokenType::Star,
                    },
                    '/' => {
                        match self.peek() {
                            Some('/') => {
                                self.skip_comment_line();
                                continue 'outer; // let next iteration handle \n
                            }
                            Some('*') => {
                                self.advance();
                                match self.skip_comment_block() {
                                    Ok(()) => continue 'outer,
                                    Err(msg) => TokenType::Illegal(msg),
                                }
                            }
                            Some('=') => {
                                self.advance();
                                TokenType::SlashEqual
                            }
                            _ => TokenType::Slash,
                        }
                    }
                    '%' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::PercentEqual
                        }
                        _ => TokenType::Percent,
                    },
                    '(' => {
                        self.bracket_depth += 1;
                        TokenType::LParen
                    }
                    ')' => {
                        self.bracket_depth -= 1;
                        TokenType::RParen
                    }
                    '{' => {
                        self.bracket_depth += 1;
                        TokenType::LBrace
                    }
                    '}' => {
                        self.bracket_depth -= 1;
                        TokenType::RBrace
                    }
                    '[' => {
                        self.bracket_depth += 1;
                        TokenType::LBracket
                    }
                    ']' => {
                        self.bracket_depth -= 1;
                        TokenType::RBracket
                    }
                    ',' => TokenType::Comma,
                    ';' => TokenType::Semicolon,
                    ':' => TokenType::Colon,
                    '.' => match self.peek() {
                        Some('.') => {
                            self.advance();
                            if self.peek() == Some('.') {
                                self.advance();
                                TokenType::DotDotDot
                            } else {
                                TokenType::DotDot
                            }
                        }
                        _ => TokenType::Dot,
                    },
                    '=' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::EqualEqual
                        }
                        Some('>') => {
                            self.advance();
                            TokenType::FatArrow
                        }
                        _ => TokenType::Equal,
                    },
                    '!' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::BangEqual
                        }
                        _ => TokenType::Bang,
                    },
                    '<' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::LessEqual
                        }
                        Some('<') => {
                            self.advance();
                            TokenType::ShiftLeft
                        }
                        _ => TokenType::Less,
                    },
                    '>' => match self.peek() {
                        Some('=') => {
                            self.advance();
                            TokenType::GreaterEqual
                        }
                        Some('>') => {
                            self.advance();
                            TokenType::ShiftRight
                        }
                        _ => TokenType::Greater,
                    },
                    '&' => match self.peek() {
                        Some('&') => {
                            self.advance();
                            TokenType::AndAnd
                        }
                        _ => TokenType::Ampersand,
                    },
                    '?' => TokenType::Question,
                    '~' => TokenType::Tilde,
                    '^' => TokenType::Caret,
                    '\\' => TokenType::Backslash,
                    '|' => match self.peek() {
                        Some('|') => {
                            self.advance();
                            TokenType::OrOr
                        }
                        Some('>') => {
                            self.advance();
                            TokenType::PipeArrow
                        }
                        _ => TokenType::Pipe,
                    },
                    '"' => {
                        let saved = self.pos;
                        let mut has_interp = false;
                        let mut escaped = false;
                        while let Some(ch) = self.chars.get(self.pos).copied() {
                            self.pos += 1;
                            if escaped {
                                escaped = false;
                                continue;
                            }
                            if ch == '\\' {
                                escaped = true;
                                continue;
                            }
                            if ch == '"' {
                                break;
                            }
                            if ch == '{' {
                                has_interp = true;
                            }
                        }
                        self.pos = saved;
                        if has_interp {
                            return self.read_interpolated_string();
                        }
                        let st = match self.read_string() {
                            Ok(t) => t,
                            Err(e) => TokenType::Illegal(e),
                        };
                        return self.tok(st);
                    }
                    ch if ch.is_ascii_digit() => self.read_number(ch),
                    ch if ch.is_alphabetic() || ch == '_' || ch == '$' => self.read_identifier(ch),
                    ch => TokenType::Illegal(format!("Unexpected character '{}'", ch)),
                },
            };

            return self.tok(token_type);
        }
    }

    fn read_number(&mut self, first: char) -> TokenType {
        let mut num_str = String::new();
        num_str.push(first);
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else if ch == '.' && !is_float && self.peek_next() != Some('.') {
                is_float = true;
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            let value: f64 = num_str.parse().unwrap_or(0.0);
            TokenType::Float(value)
        } else {
            let value: i64 = num_str.parse().unwrap_or(0);
            TokenType::Int(value)
        }
    }

    fn read_string(&mut self) -> Result<TokenType, String> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("Unterminated string".to_string()),
                Some('"') => break,
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('"') => s.push('"'),
                    Some('\\') => s.push('\\'),
                    Some(c) => s.push(c),
                    None => return Err("Unterminated string escape".to_string()),
                },
                Some(ch) => s.push(ch),
            }
        }
        Ok(TokenType::String(s))
    }

    fn read_interpolated_string(&mut self) -> Token {
        let mut literal = String::new();
        let mut pending_tokens: Vec<Token> = Vec::new();

        loop {
            match self.advance() {
                None => return self.tok(TokenType::Illegal("Unterminated string".into())),
                Some('"') => {
                    pending_tokens.push(self.tok(TokenType::StringPart(literal)));
                    pending_tokens.push(self.tok(TokenType::StringEnd));
                    break;
                }
                Some('\\') => match self.advance() {
                    Some('n') => literal.push('\n'),
                    Some('t') => literal.push('\t'),
                    Some('"') => literal.push('"'),
                    Some('\\') => literal.push('\\'),
                    Some('{') => literal.push('{'),
                    Some(c) => literal.push(c),
                    None => return self.tok(TokenType::Illegal("Unterminated escape".into())),
                },
                Some('{') => {
                    pending_tokens.push(self.tok(TokenType::StringPart(literal)));
                    literal = String::new();
                    let mut expr_text = String::new();
                    let mut depth = 1;
                    let mut in_str = false;
                    loop {
                        match self.advance() {
                            None => {
                                return self
                                    .tok(TokenType::Illegal("Unterminated interpolation".into()))
                            }
                            Some('\\') => match self.advance() {
                                Some('"') => {
                                    in_str = !in_str;
                                    expr_text.push('"');
                                }
                                Some('{') => {
                                    expr_text.push('{');
                                }
                                Some('}') => {
                                    expr_text.push('}');
                                }
                                Some(c) => {
                                    expr_text.push(c);
                                }
                                None => {
                                    return self
                                        .tok(TokenType::Illegal("Unterminated escape".into()))
                                }
                            },
                            Some('"') => {
                                in_str = !in_str;
                                expr_text.push('"');
                            }
                            Some('{') if !in_str => {
                                depth += 1;
                                expr_text.push('{');
                            }
                            Some('}') if !in_str => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                expr_text.push('}');
                            }
                            Some(c) => expr_text.push(c),
                        }
                    }
                    pending_tokens.push(self.tok(TokenType::ExprString(expr_text)));
                }
                Some(ch) => literal.push(ch),
            }
        }

        for t in pending_tokens.into_iter().rev() {
            self.pending.push(t);
        }
        self.pending.pop().unwrap_or(self.tok(TokenType::StringEnd))
    }

    fn read_raw_string(&mut self) -> TokenType {
        let mut content = String::new();
        loop {
            match self.advance() {
                None => return TokenType::Illegal("Unterminated raw string".into()),
                Some('"') => return TokenType::String(content),
                Some(ch) => content.push(ch),
            }
        }
    }

    fn read_identifier(&mut self, first: char) -> TokenType {
        let mut ident = String::new();
        ident.push(first);
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        // Raw string literal: r"..."
        if ident == "r" && self.peek() == Some('"') {
            self.advance(); // consume opening "
            return self.read_raw_string();
        }
        match ident.as_str() {
            "async" => TokenType::Async,
            "await" => TokenType::Await,
            "cell" => TokenType::Cell,
            "say" => TokenType::Say,
            "shout" => TokenType::Shout,
            "emit" => TokenType::Emit,
            "when" => TokenType::When,
            "else" => TokenType::Else,
            "while" => TokenType::While,
            "over" => TokenType::Each,
            "in" => TokenType::In,
            "pick" => TokenType::Pick,
            "dare" => TokenType::Dare,
            "catch" => TokenType::Catch,
            "load" => TokenType::Load,
            "shape" => TokenType::Shape,
            "style" => TokenType::Style,
            "class" => TokenType::Class,
            "as" => TokenType::As,
            "none" => TokenType::None,
            "break" => TokenType::Break,
            "continue" => TokenType::Continue,
            "do" => TokenType::Do,
            "trait" => TokenType::Trait,
            "impl" => TokenType::Impl,
            "for" => TokenType::For,
            "yield" => TokenType::Yield,
            "macro" => TokenType::Macro,
            "pub" => TokenType::Pub,
            "extends" => TokenType::Extends,
            "throw" => TokenType::Throw,
            "super" => TokenType::Super,
            "yes" => TokenType::Boolean(true),
            "no" => TokenType::Boolean(false),
            "int" => TokenType::TypeInt,
            "float" => TokenType::TypeFloat,
            "string" => TokenType::TypeString,
            "bool" => TokenType::TypeBool,
            "list" => TokenType::TypeList,
            "dict" => TokenType::TypeDict,
            "Self" => TokenType::TypeSelf,
            _ => TokenType::Identifier(ident),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<TokenType> {
        Lexer::new(source)
            .tokenize()
            .into_iter()
            .map(|t| t.token_type)
            .collect()
    }

    #[test]
    fn test_numbers() {
        let toks = tokenize("42 3.14");
        assert_eq!(toks[0], TokenType::Int(42));
        assert_eq!(toks[1], TokenType::Float(3.14));
        assert_eq!(toks[2], TokenType::Eof);
    }

    #[test]
    fn test_strings() {
        let toks = tokenize("\"hello\"");
        assert_eq!(toks[0], TokenType::String("hello".into()));
    }

    #[test]
    fn test_keywords() {
        let toks = tokenize("cell say shout emit when else while over in pick dare catch load shape style as none break continue do");
        assert_eq!(toks[0], TokenType::Cell);
        assert_eq!(toks[1], TokenType::Say);
        assert_eq!(toks[2], TokenType::Shout);
        assert_eq!(toks[3], TokenType::Emit);
        assert_eq!(toks[4], TokenType::When);
        assert_eq!(toks[5], TokenType::Else);
        assert_eq!(toks[6], TokenType::While);
        assert_eq!(toks[7], TokenType::Each); // over
        assert_eq!(toks[8], TokenType::In);
        assert_eq!(toks[9], TokenType::Pick);
        assert_eq!(toks[10], TokenType::Dare);
        assert_eq!(toks[11], TokenType::Catch);
        assert_eq!(toks[12], TokenType::Load);
        assert_eq!(toks[13], TokenType::Shape);
        assert_eq!(toks[14], TokenType::Style);
        assert_eq!(toks[15], TokenType::As);
        assert_eq!(toks[16], TokenType::None);
        assert_eq!(toks[17], TokenType::Break);
        assert_eq!(toks[18], TokenType::Continue);
    }

    #[test]
    fn test_booleans() {
        let toks = tokenize("yes no");
        assert_eq!(toks[0], TokenType::Boolean(true));
        assert_eq!(toks[1], TokenType::Boolean(false));
    }

    #[test]
    fn test_operators() {
        let toks = tokenize("+ - * / % += -= *= /= %= == != < <= > >= && || !");
        assert!(toks.contains(&TokenType::Plus));
        assert!(toks.contains(&TokenType::Percent));
        assert!(toks.contains(&TokenType::PlusEqual));
        assert!(toks.contains(&TokenType::PercentEqual));
        assert!(toks.contains(&TokenType::EqualEqual));
        assert!(toks.contains(&TokenType::GreaterEqual));
        assert!(toks.contains(&TokenType::AndAnd));
        assert!(toks.contains(&TokenType::OrOr));
    }

    #[test]
    fn test_comments() {
        let toks = tokenize("// line comment\n42 /* block */ 3");
        assert_eq!(toks[0], TokenType::Newline);
        assert_eq!(toks[1], TokenType::Int(42));
        assert_eq!(toks[2], TokenType::Int(3));
    }

    #[test]
    fn test_identifiers() {
        let toks = tokenize("foo bar_123");
        assert_eq!(toks[0], TokenType::Identifier("foo".into()));
        assert_eq!(toks[1], TokenType::Identifier("bar_123".into()));
    }

    #[test]
    fn test_range_arrow() {
        let toks = tokenize(".. ->");
        assert_eq!(toks[0], TokenType::DotDot);
        assert_eq!(toks[1], TokenType::Arrow);
    }

    #[test]
    fn test_string_escapes() {
        let toks = tokenize("\"a\\nb\\tc\"");
        assert_eq!(toks[0], TokenType::String("a\nb\tc".into()));
    }

    #[test]
    fn test_raw_string() {
        let toks = tokenize("r\"a\\nb\\tc\"");
        assert_eq!(toks[0], TokenType::String("a\\nb\\tc".into()));
    }

    #[test]
    fn test_raw_string_no_escapes() {
        let toks = tokenize("r\"hello\\nworld\"");
        assert_eq!(toks[0], TokenType::String("hello\\nworld".into()));
    }

    #[test]
    fn test_raw_string_quotes() {
        // raw string stops at the first ", backslash doesn't escape
        let toks = tokenize("r\"she said \"");
        assert_eq!(toks[0], TokenType::String("she said ".into()));
    }
}
