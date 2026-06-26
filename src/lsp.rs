use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use crate::ast::{DestructureItem, DestructureTarget, Stmt};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::typeck::TypeChecker;

struct Document {
    #[allow(dead_code)]
    uri: String,
    text: String,
    #[allow(dead_code)]
    version: i64,
}

pub fn run_lsp() -> ! {
    let mut server = Server::new();
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let Some(msg) = read_message(&mut stdin.lock()) else { break };
        if !server.handle_message(&msg, &mut stdout) {
            break;
        }
    }
    std::process::exit(0);
}

fn read_message(stdin: &mut io::StdinLock<'_>) -> Option<String> {
    let mut header = String::new();
    let mut content_length = 0usize;

    loop {
        header.clear();
        stdin.read_line(&mut header).ok()?;
        let h = header.trim();
        if h.is_empty() { break; }
        if let Some(len_str) = h.strip_prefix("Content-Length: ") {
            content_length = len_str.trim().parse().ok()?;
        }
    }

    if content_length == 0 { return None; }

    let mut body = vec![0u8; content_length];
    let mut read = 0;
    while read < content_length {
        match stdin.read(&mut body[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => break,
        }
    }
    String::from_utf8(body).ok()
}

fn write_message(stdout: &mut io::Stdout, body: &str) {
    let _ = writeln!(stdout, "Content-Length: {}\r\n", body.len());
    let _ = writeln!(stdout, "{}", body);
    let _ = stdout.flush();
}

struct Server {
    documents: HashMap<String, Document>,
    #[allow(dead_code)]
    next_id: i64,
}

impl Server {
    fn new() -> Self {
        Server { documents: HashMap::new(), next_id: 1 }
    }

    fn handle_message(&mut self, body: &str, stdout: &mut io::Stdout) -> bool {
        if let Some(id) = get_json_field(body, "\"id\":") {
            let id = id.trim_end_matches(|c: char| !c.is_ascii_digit());
            let id = id.parse::<i64>().unwrap_or(0);

            let method = get_json_str(body, "\"method\":");
            let method = method.as_deref().unwrap_or("").to_string();

            match method.as_str() {
                "initialize" => self.handle_initialize(id, stdout),
                "textDocument/completion" => self.handle_completion(id, body, stdout),
                "textDocument/hover" => self.handle_hover(id, body, stdout),
                "textDocument/definition" => self.handle_definition(id, body, stdout),
                "shutdown" => { self.send_result(id, "null", stdout); return false; }
                _ => {
                    let msg = format!(r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32601,"message":"Method not found: {}"}}}}"#, id, method);
                    write_message(stdout, &msg);
                }
            }
        } else {
            let method = get_json_str(body, "\"method\":");
            let method = method.as_deref().unwrap_or("").to_string();

            match method.as_str() {
                "initialized" => {}
                "textDocument/didOpen" => self.handle_did_open(body),
                "textDocument/didChange" => self.handle_did_change(body),
                "textDocument/didSave" => {}
                "$cancelRequest" => {}
                _ => {}
            }
        }
        true
    }

    fn handle_initialize(&mut self, id: i64, stdout: &mut io::Stdout) {
        let capabilities = r#"{
            "capabilities":{
                "textDocumentSync":2,
                "completionProvider":{"triggerCharacters":[".","("," "]},
                "hoverProvider":true,
                "definitionProvider":true
            }
        }"#;
        let msg = format!(r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#, id, capabilities);
        write_message(stdout, &msg);
    }

    fn handle_did_open(&mut self, body: &str) {
        let uri = get_json_nested_str(body, "\"textDocument\":", "\"uri\":")
            .unwrap_or_default();
        let text = get_json_nested_str(body, "\"textDocument\":", "\"text\":")
            .unwrap_or_default();
        self.documents.insert(uri.clone(), Document { uri: uri.clone(), text, version: 0 });
        let mut stdout = io::stdout();
        self.send_diagnostics(&uri, &mut stdout);
    }

    fn handle_did_change(&mut self, body: &str) {
        let uri = get_json_nested_str(body, "\"textDocument\":", "\"uri\":")
            .unwrap_or_default();
        if let Some(doc) = self.documents.get_mut(&uri)
            && let Some(text) = get_json_nested_str(body, "\"contentChanges\":", "\"text\":") {
            doc.text = text;
        }
        let mut stdout = io::stdout();
        self.send_diagnostics(&uri, &mut stdout);
    }

    fn send_result(&self, id: i64, result: &str, stdout: &mut io::Stdout) {
        let msg = format!(r#"{{"jsonrpc":"2.0","id":{},"result":{}}}"#, id, result);
        write_message(stdout, &msg);
    }

    fn send_diagnostics(&self, uri: &str, stdout: &mut io::Stdout) {
        let Some(doc) = self.documents.get(uri) else { return };
        let diags = compute_diagnostics(&doc.text);
        let msg = format!(
            r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":"{}","diagnostics":[{}]}}}}"#,
            uri, diags
        );
        write_message(stdout, &msg);
    }

    fn handle_completion(&mut self, id: i64, body: &str, stdout: &mut io::Stdout) {
        let uri = get_json_nested_str(body, "\"textDocument\":", "\"uri\":").unwrap_or_default();
        let cursor_line = get_json_field(body, "\"line\":")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let cursor_char = get_json_field(body, "\"character\":")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);

        let items = compute_completions(
            self.documents.get(&uri).map(|d| d.text.as_str()).unwrap_or(""),
            cursor_line, cursor_char,
        );

        let result = format!(r#"{{"isIncomplete":false,"items":[{}]}}"#, items);
        self.send_result(id, &result, stdout);
    }

    fn handle_hover(&mut self, id: i64, body: &str, stdout: &mut io::Stdout) {
        let uri = get_json_nested_str(body, "\"textDocument\":", "\"uri\":").unwrap_or_default();
        let cursor_line = get_json_field(body, "\"line\":")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let cursor_char = get_json_field(body, "\"character\":")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);

        let info = compute_hover(
            self.documents.get(&uri).map(|d| d.text.as_str()).unwrap_or(""),
            cursor_line, cursor_char,
        );

        self.send_result(id, &info, stdout);
    }

    fn handle_definition(&mut self, id: i64, body: &str, stdout: &mut io::Stdout) {
        let uri = get_json_nested_str(body, "\"textDocument\":", "\"uri\":").unwrap_or_default();
        let cursor_line = get_json_field(body, "\"line\":")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let cursor_char = get_json_field(body, "\"character\":")
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);

        let def = compute_definition(
            self.documents.get(&uri).map(|d| d.text.as_str()).unwrap_or(""),
            &uri, cursor_line, cursor_char,
        );

        self.send_result(id, &def, stdout);
    }
}

fn get_json_field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let idx = body.find(key)?;
    let start = idx + key.len();
    let rest = &body[start..];
    let trimmed = rest.trim_start();
    if let Some(stripped) = trimmed.strip_prefix('"') {
        // string value
        let mut escaped = false;
        for (i, ch) in stripped.char_indices() {
            if escaped { escaped = false; continue; }
            if ch == '\\' { escaped = true; continue; }
            if ch == '"' {
                return Some(&trimmed[1..=i]);
            }
        }
        None
    } else {
        // number or bool or null
        let end = trimmed.find([',', '}', ']']).unwrap_or(trimmed.len());
        Some(trimmed[..end].trim())
    }
}

fn get_json_str(body: &str, key: &str) -> Option<String> {
    get_json_field(body, key).map(|s| {
        // unescape JSON string
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(c) => { out.push('\\'); out.push(c); }
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    })
}

fn get_json_nested_str(body: &str, outer_key: &str, inner_key: &str) -> Option<String> {
    let outer_start = body.find(outer_key)?;
    let rest = &body[outer_start + outer_key.len()..];
    // Find the object value
    let obj_start = rest.find('{')?;
    let mut depth = 0;
    let mut obj_end = obj_start;
    for (i, ch) in rest[obj_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => { depth -= 1; if depth == 0 { obj_end = obj_start + i + 1; break; } }
            _ => {}
        }
    }
    let obj_str = &rest[obj_start..obj_end];
    get_json_str(obj_str, inner_key)
}

fn compute_diagnostics(text: &str) -> String {
    let mut diags = Vec::new();

    // Lex errors
    let mut lexer = Lexer::new(text);
    let tokens = lexer.tokenize();
    let mut has_lex_errors = false;
    for t in &tokens {
        if let crate::token::TokenType::Illegal(msg) = &t.token_type {
            diags.push(format!(
                r#"{{"range":{{"start":{{"line":{},"character":0}},"end":{{"line":{},"character":100}}}},"severity":1,"message":"Lex error: {}","source":"elitra"}}"#,
                t.line - 1, t.line - 1, msg
            ));
            has_lex_errors = true;
        }
    }

    if has_lex_errors {
        return diags.join(",");
    }

    // Parse errors
    let mut parser = Parser::new(tokens);
    match parser.parse() {
        Ok(stmts) => {
            // Type errors
            let mut checker = TypeChecker::new();
            if let Err(errs) = checker.check(&stmts) {
                for d in errs {
                    let line = if d.line > 0 { d.line - 1 } else { 0 };
                    diags.push(format!(
                        r#"{{"range":{{"start":{{"line":{},"character":0}},"end":{{"line":{},"character":100}}}},"severity":1,"message":"{}","source":"elitra"}}"#,
                        line, line, d.msg
                    ));
                }
            }
        }
        Err(e) => {
            // Try to extract line number from error message
            let line = e.split_whitespace()
                .find_map(|w| w.parse::<usize>().ok())
                .unwrap_or(0);
            let line = if line > 0 { line - 1 } else { 0 };
            diags.push(format!(
                r#"{{"range":{{"start":{{"line":{},"character":0}},"end":{{"line":{},"character":100}}}},"severity":1,"message":"{}","source":"elitra"}}"#,
                line, line, e
            ));
        }
    }

    diags.join(",")
}

#[allow(dead_code)]
fn get_word_at_pos(text: &str, line: usize, character: usize) -> &str {
    let source_line = text.lines().nth(line).unwrap_or("");
    let start = character.saturating_sub(1);
    let end = character;
    if end > source_line.len() { return ""; }
    &source_line[start..end]
}

fn compute_completions(text: &str, _line: usize, _character: usize) -> String {
    let keywords = [
        ("cell", "keyword"), ("say", "keyword"), ("shout", "keyword"), ("emit", "keyword"),
        ("when", "keyword"), ("else", "keyword"), ("while", "keyword"),         ("over", "keyword"),
        ("in", "keyword"), ("pick", "keyword"), ("dare", "keyword"), ("catch", "keyword"),
        ("load", "keyword"), ("shape", "keyword"), ("style", "keyword"), ("as", "keyword"),
        ("none", "keyword"), ("break", "keyword"), ("continue", "keyword"),
        ("async", "keyword"), ("await", "keyword"),
        ("yes", "constant"), ("no", "constant"),
    ];

    let builtins = [
        ("len", "function"), ("str", "function"), ("int", "function"), ("float", "function"),
        ("bool", "function"), ("type", "function"), ("input", "function"),
        ("abs", "function"), ("sin", "function"), ("cos", "function"), ("sqrt", "function"),
        ("read", "function"), ("write", "function"), ("lines", "function"),
        ("assert", "function"), ("clock", "function"), ("exit", "function"),
        ("push", "function"), ("pop", "function"), ("sort", "function"), ("reverse", "function"),
        ("join", "function"), ("split", "function"), ("trim", "function"),
        ("upper", "function"), ("lower", "function"), ("contains", "function"),
        ("replace", "function"), ("floor", "function"), ("ceil", "function"),
        ("round", "function"), ("max", "function"), ("min", "function"),
        ("pow", "function"), ("log", "function"), ("exp", "function"),
        ("json_encode", "function"), ("json_decode", "function"), ("json_validate", "function"),
        ("map", "function"), ("filter", "function"), ("fold", "function"),
        ("take", "function"), ("collect", "function"), ("iter", "function"),
        ("set", "function"),
        ("Ok", "function"), ("Err", "function"), ("Some", "function"),
    ];

    let types = [
        ("int", "type"), ("float", "type"), ("string", "type"),
        ("bool", "type"), ("list", "type"), ("dict", "type"),
        ("Result", "type"), ("Option", "type"), ("none", "type"), ("any", "type"),
    ];

    let mut items = Vec::new();

    for (label, kind) in &keywords {
        items.push(format!(
            r#"{{"label":"{}","kind":14,"detail":"{}"}}"#,
            label, kind
        ));
    }

    for (label, kind) in &builtins {
        items.push(format!(
            r#"{{"label":"{}","kind":3,"detail":"{}"}}"#,
            label, kind
        ));
    }

    for (label, kind) in &types {
        items.push(format!(
            r#"{{"label":"{}","kind":22,"detail":"{}"}}"#,
            label, kind
        ));
    }

    // Extract user-defined names from source
    let mut lexer = Lexer::new(text);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    if let Ok(stmts) = parser.parse() {
        let names = extract_definitions(&stmts);
        for name in names {
            items.push(format!(
                r#"{{"label":"{}","kind":6,"detail":"user"}}"#,
                name
            ));
        }
    }

    items.join(",")
}

fn extract_definitions(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for s in stmts {
        match s {
            Stmt::Let { name, .. } => names.push(name.clone()),
            Stmt::Fn { name, .. } => names.push(name.clone()),
            Stmt::Struct { name, .. } => names.push(name.clone()),
            Stmt::Enum { name, .. } => names.push(name.clone()),
            Stmt::Destructure { target, .. } => {
                match target {
                    DestructureTarget::List(items) => {
                        for item in items {
                            match item {
                                DestructureItem::Name(n) => names.push(n.clone()),
                                DestructureItem::Rest(n) => names.push(n.clone()),
                            }
                        }
                    }
                    DestructureTarget::Struct(fields) => {
                        for f in fields {
                            names.push(f.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    names
}

fn compute_hover(text: &str, line: usize, character: usize) -> String {
    let source_line = text.lines().nth(line).unwrap_or("");

    // Find the word at cursor
    let before = &source_line[..character.min(source_line.len())];
    let after = &source_line[character.min(source_line.len())..];
    let word_start = before.rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1).unwrap_or(0);
    let word_end = after.find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| character + i).unwrap_or(source_line.len());
    let word = &source_line[word_start..word_end];

    if word.is_empty() {
        return "null".into();
    }

    // Try to figure out what this is
    let detail = if is_keyword(word) {
        format!("keyword `{}`", word)
    } else if is_builtin(word) {
        format!("builtin function `{}`", word)
    } else if is_type(word) {
        format!("type `{}`", word)
    } else if let Some(t) = find_type(text, word) {
        format!("`{}`: {}", word, t)
    } else {
        format!("`{}`", word)
    };

    format!(r#"{{"contents":{{"kind":"markdown","value":"{}"}}}}"#, detail)
}

#[allow(dead_code)]
fn compute_hover_type(_text: &str, _name: &str) -> Option<String> {
    None
}

fn compute_definition(text: &str, _uri: &str, line: usize, character: usize) -> String {
    let source_line = text.lines().nth(line).unwrap_or("");
    let before = &source_line[..character.min(source_line.len())];
    let after = &source_line[character.min(source_line.len())..];
    let word_start = before.rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1).unwrap_or(0);
    let word_end = after.find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| character + i).unwrap_or(source_line.len());
    let word = &source_line[word_start..word_end];

    if word.is_empty() {
        return "null".into();
    }

    // Search for definition in the source
    for (i, line_text) in text.lines().enumerate() {
        let trimmed = line_text.trim();
        // Check cell x =, fn x(, shape x, style x, cell x:
        if trimmed.starts_with(&format!("cell {} ", word))
            || trimmed.starts_with(&format!("cell {}=", word))
            || trimmed.starts_with(&format!("cell {}:", word))
            || trimmed.starts_with(&format!("fn {}(", word))
            || trimmed.starts_with(&format!("fn {} (", word))
            || trimmed.starts_with(&format!("~{}(", word))
            || trimmed.starts_with(&format!("shape {}", word))
            || trimmed.starts_with(&format!("style {}", word))
        {
            let col = line_text.find(word).unwrap_or(0);
            return format!(
                r#"{{"uri":"{}","range":{{"start":{{"line":{},"character":{}}},"end":{{"line":{},"character":{}}}}}}}"#,
                _uri, i, col, i, col + word.len()
            );
        }
    }

    "null".into()
}

fn is_keyword(s: &str) -> bool {
    matches!(s, "cell" | "say" | "shout" | "emit" | "when" | "else" | "while" | "over" | "in"
        | "pick" | "dare" | "catch" | "load" | "shape" | "style" | "as"
        | "none" | "break" | "continue" | "async" | "await" | "yes" | "no")
}

fn is_builtin(s: &str) -> bool {
    matches!(s, "len" | "str" | "int" | "float" | "bool" | "type" | "input"
        | "abs" | "sin" | "cos" | "sqrt" | "read" | "write" | "lines"
        | "assert" | "clock" | "exit" | "push" | "pop" | "sort" | "reverse"
        | "join" | "split" | "trim" | "upper" | "lower" | "contains" | "replace"
        | "floor" | "ceil" | "round" | "max" | "min" | "pow" | "log" | "exp"
        | "json_encode" | "json_decode" | "json_validate"
        | "map" | "filter" | "fold" | "take" | "collect" | "iter" | "set"
        | "Ok" | "Err" | "Some")
}

fn is_type(s: &str) -> bool {
    matches!(s, "int" | "float" | "string" | "bool" | "list" | "dict"
        | "Result" | "Option" | "none" | "any")
}

fn find_type(text: &str, _name: &str) -> Option<String> {
    let tokens = Lexer::new(text).tokenize();
    let mut parser = Parser::new(tokens);
    let stmts = parser.parse().ok()?;
    let mut checker = TypeChecker::new();
    checker.check_old(&stmts).ok()?;
    // We can't easily get the type of a variable from TypeChecker
    // since it doesn't expose symbol table lookups.
    // For now, just return None.
    None
}
