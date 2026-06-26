mod token;
mod lexer;
mod ast;
mod parser;
mod typeck;
mod interpreter;
mod error;
mod package;
mod lsp;
mod fmt;
mod test_runner;
mod jit;
mod jit_rt;

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use error::{Error, ErrorKind, Result};
use lexer::Lexer;
use parser::Parser;
use typeck::TypeChecker;
use interpreter::debugger::Debugger;
use interpreter::Interpreter;

const VERSION: &str = "1.3.0";

fn run(source: &str, interpreter: &mut Interpreter, check: bool) -> Result<()> {
    if interpreter.jit.is_none() {
        interpreter.jit = Some(jit::JitEngine::new());
    }
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    let statements = parser.parse()
        .map_err(|e| Error::new(ErrorKind::Parse, e))?;

    if check {
        let mut checker = TypeChecker::new();
        checker.check_old(&statements)
            .map_err(|errs| Error::new(ErrorKind::TypeError, errs.join("\n")))?;
    }

    interpreter.interpret(&statements)?;
    Ok(())
}

fn run_file(path: &str, type_check: bool) -> Result<()> {
    run_file_debug(path, type_check, false)
}

fn run_file_debug(path: &str, type_check: bool, debug: bool) -> Result<()> {
    let source = fs::read_to_string(Path::new(path))
        .map_err(|e| Error::new(ErrorKind::IO, format!("Could not read file '{}': {}", path, e)))?;
    let mut interpreter = Interpreter::new();
    interpreter.type_check = type_check;
    interpreter.jit_enabled = !debug;
    if debug {
        interpreter.debugger = Some(Debugger::new());
    }
    interpreter.set_current_file(path);
    run(&source, &mut interpreter, type_check)
}

fn repl() -> Result<()> {
    let mut interpreter = Interpreter::new();
    interpreter.jit_enabled = true;
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!("Elitra Lang v{}", VERSION);
    println!("Type 'exit' to quit");

    let mut buffer = String::new();
    loop {
        if buffer.is_empty() {
            print!("> ");
        } else {
            print!(".. ");
        }
        stdout.flush().map_err(|e| Error::new(ErrorKind::IO, e.to_string()))?;

        let mut line = String::new();
        stdin.lock().read_line(&mut line).map_err(|e| Error::new(ErrorKind::IO, e.to_string()))?;

        let trimmed = line.trim();
        if trimmed.is_empty() && !buffer.is_empty() {
            continue;
        }
        if trimmed == "exit" {
            break;
        }

        buffer.push_str(line.as_str());

        if is_complete(&buffer) {
            match run(buffer.trim(), &mut interpreter, false) {
                Ok(()) => {}
                Err(e) => {
                    println!("{}", format_error(&buffer, &e));
                }
            }
            buffer.clear();
        }
    }

    Ok(())
}

fn is_complete(source: &str) -> bool {
    let mut braces = 0i64;
    let mut parens = 0i64;
    let mut in_string = false;
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            if ch == '\\' { chars.next(); }
            else if ch == '"' { in_string = false; }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => braces += 1,
            '}' => braces -= 1,
            '(' => parens += 1,
            ')' => parens -= 1,
            '/' if chars.peek() == Some(&'/') => {
                while chars.next().is_some_and(|c| c != '\n') {}
            }
            _ => {}
        }
    }
    braces <= 0 && parens <= 0
}

fn format_error(source: &str, err: &Error) -> String {
    if let Some(ref loc) = err.loc
        && loc.line > 0
    {
        let lines: Vec<&str> = source.lines().collect();
        if loc.line <= lines.len() {
            let line = lines[loc.line - 1];
            let col = loc.col.min(line.len());
            return format!(
                "{} at {}:\n  {}\n  {}{}\n{}",
                err.kind_label(),
                loc,
                line,
                " ".repeat(col),
                "^".repeat((line.len() - col).max(1)),
                err.msg,
            );
        }
    }
    format!("[{}] {}", err.kind_label(), err.msg)
}

fn print_help() {
    println!("eltr v{}", VERSION);
    println!();
    println!("Usage: eltr [COMMAND] [OPTIONS] [FILE]");
    println!();
    println!("Commands:");
    println!("  run [FILE]      Execute a script file (default if no command given)");
    println!("                  Without FILE, runs project from package.toml");
    println!("  debug [FILE]    Execute with interactive debugger");
    println!("  fmt [FILE]      Format a script file (in-place)");
    println!("  fmt --check [FILE]  Check formatting without modifying");
    println!("  test [FILE]     Run tests in a script file");
    println!("  lsp             Start LSP server (stdin/stdout JSON-RPC)");
    println!("  init [name]     Create a new project");
    println!("  build           Check types in the current project");
    println!("  install <pkg>   Install a package");
    println!();
    println!("Options:");
    println!("  --check-types   Enable type checking");
    println!("  --debug, -d     Run with interactive debugger");
    println!("  --help, -h      Show this help message");
    println!("  --version, -v   Show version information");
    println!();
    println!("Run without arguments to start the REPL.");
}

fn do_fmt(path: &str, check: bool) -> Result<()> {
    let source = fs::read_to_string(Path::new(path))
        .map_err(|e| Error::new(ErrorKind::IO, format!("Could not read '{}': {}", path, e)))?;
    let formatted = fmt::format_source(&source)
        .map_err(|e| Error::new(ErrorKind::Parse, e))?;
    if check {
        if source != formatted {
            return Err(Error::new(ErrorKind::IO, "File is not formatted"));
        }
        println!("File is formatted");
    } else {
        fs::write(Path::new(path), &formatted)
            .map_err(|e| Error::new(ErrorKind::IO, format!("Could not write '{}': {}", path, e)))?;
        println!("Formatted {}", path);
    }
    Ok(())
}

fn do_test(path: &str) -> Result<()> {
    let source = fs::read_to_string(Path::new(path))
        .map_err(|e| Error::new(ErrorKind::IO, format!("Could not read '{}': {}", path, e)))?;
    let results = test_runner::run_tests(&source, Some(path))
        .map_err(|e| Error::new(ErrorKind::Runtime, e))?;
    test_runner::print_results(&results);
    if !results.failed.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

struct PackageMeta {
    name: String,
    version: String,
    description: String,
    entry: String,
    deps: HashMap<String, String>,
}

fn parse_package_toml(content: &str) -> std::result::Result<PackageMeta, String> {
    let mut meta = PackageMeta {
        name: String::new(),
        version: String::new(),
        description: String::new(),
        entry: "src/main.eltr".to_string(),
        deps: HashMap::new(),
    };
    let mut in_deps = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[dependencies]" {
            in_deps = true;
            continue;
        }
        if line.starts_with('[') {
            in_deps = false;
            continue;
        }
        if in_deps {
            if let Some((name, ver)) = line.split_once('=') {
                let k = name.trim().to_string();
                let v = ver.trim().trim_matches('"').trim_matches('\'').to_string();
                if !k.is_empty() {
                    meta.deps.insert(k, v);
                }
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let k = key.trim();
            let v = value.trim().trim_matches('"');
            match k {
                "name" => meta.name = v.to_string(),
                "version" => meta.version = v.to_string(),
                "description" => meta.description = v.to_string(),
                "entry" => meta.entry = v.to_string(),
                _ => {}
            }
        }
    }
    if meta.name.is_empty() {
        return Err("package.toml missing 'name'".into());
    }
    Ok(meta)
}

fn fmt_package_toml(meta: &PackageMeta) -> String {
    let mut s = String::new();
    s.push_str(&format!("name = \"{}\"\n", meta.name));
    s.push_str(&format!("version = \"{}\"\n", meta.version));
    if !meta.description.is_empty() {
        s.push_str(&format!("description = \"{}\"\n", meta.description));
    }
    s.push_str(&format!("entry = \"{}\"\n", meta.entry));
    if !meta.deps.is_empty() {
        s.push_str("\n[dependencies]\n");
        let mut deps: Vec<_> = meta.deps.iter().collect();
        deps.sort_by(|a, b| a.0.cmp(b.0));
        for (name, ver) in deps {
            s.push_str(&format!("{} = \"{}\"\n", name, ver));
        }
    }
    s
}

fn find_package_toml() -> std::result::Result<PathBuf, String> {
    let candidates = ["package.toml", "Package.toml"];
    for c in &candidates {
        let p = Path::new(c);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }
    Err("no package.toml found in current directory".into())
}

fn cmd_init(args: &[String]) -> Result<()> {
    let raw = args.first().cloned().unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|d| d.file_name().and_then(|n| n.to_str().map(String::from)))
            .unwrap_or_else(|| "project".to_string())
    });

    let dir = Path::new(&raw);
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or(&raw).to_string();

    if dir.exists() {
        return Err(Error::new(ErrorKind::IO, format!("'{}' already exists", name)));
    }

    fs::create_dir_all(dir.join("src")).map_err(|e| Error::new(ErrorKind::IO, format!("could not create src/: {}", e)))?;

    let meta = PackageMeta {
        name: name.clone(),
        version: "0.1.0".to_string(),
        description: String::new(),
        entry: "src/main.eltr".to_string(),
        deps: HashMap::new(),
    };

    let toml_content = fmt_package_toml(&meta);
    fs::write(dir.join("package.toml"), &toml_content).map_err(|e| Error::new(ErrorKind::IO, format!("could not write package.toml: {}", e)))?;

    let main_content = "shout \"Hello from new project!\"\n";
    fs::write(dir.join("src/main.eltr"), main_content).map_err(|e| Error::new(ErrorKind::IO, format!("could not write src/main.eltr: {}", e)))?;

    println!("Created project '{}'", name);
    println!("  package.toml");
    println!("  src/main.eltr");
    Ok(())
}

fn cmd_run_project() -> Result<()> {
    let path = find_package_toml().map_err(|e| Error::new(ErrorKind::IO, e))?;
    let content = fs::read_to_string(&path).map_err(|e| Error::new(ErrorKind::IO, format!("reading package.toml: {}", e)))?;
    let meta = parse_package_toml(&content).map_err(|e| Error::new(ErrorKind::Parse, e))?;
    let entry = Path::new(&meta.entry);
    if !entry.exists() {
        return Err(Error::new(ErrorKind::IO, format!("entry point '{}' not found", meta.entry)));
    }
    run_file(&meta.entry, false)
}

fn cmd_build_project() -> Result<()> {
    let path = find_package_toml().map_err(|e| Error::new(ErrorKind::IO, e))?;
    let content = fs::read_to_string(&path).map_err(|e| Error::new(ErrorKind::IO, format!("reading package.toml: {}", e)))?;
    let meta = parse_package_toml(&content).map_err(|e| Error::new(ErrorKind::Parse, e))?;
    let entry = Path::new(&meta.entry);
    if !entry.exists() {
        return Err(Error::new(ErrorKind::IO, format!("entry point '{}' not found", meta.entry)));
    }
    run_file(&meta.entry, true)?;
    println!("Build OK");
    Ok(())
}

fn cmd_install(args: &[String]) -> Result<()> {
    if args.is_empty() {
        return Err(Error::new(ErrorKind::IO, "'install' requires a package name".to_string()));
    }

    let path = find_package_toml().map_err(|e| Error::new(ErrorKind::IO, e))?;
    let content = fs::read_to_string(&path).map_err(|e| Error::new(ErrorKind::IO, format!("reading package.toml: {}", e)))?;
    let mut meta = parse_package_toml(&content).map_err(|e| Error::new(ErrorKind::Parse, e))?;

    for pkg in args {
        if meta.deps.contains_key(pkg) {
            println!("'{}' already installed", pkg);
            continue;
        }
        let packages_dir = Path::new("packages");
        if !packages_dir.exists() {
            fs::create_dir_all(packages_dir).map_err(|e| Error::new(ErrorKind::IO, format!("creating packages/ dir: {}", e)))?;
        }
        let pkg_dir = packages_dir.join(pkg);
        if !pkg_dir.exists() {
            fs::create_dir_all(&pkg_dir).map_err(|e| Error::new(ErrorKind::IO, format!("creating package dir: {}", e)))?;
        }

        meta.deps.insert(pkg.clone(), "*".to_string());
        println!("Installed '{}'", pkg);
    }

    let toml_content = fmt_package_toml(&meta);
    fs::write(&path, &toml_content).map_err(|e| Error::new(ErrorKind::IO, format!("writing package.toml: {}", e)))?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 1 {
        match repl() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("[{}] {}", e.kind_label(), e.msg);
                std::process::exit(1);
            }
        }
        return;
    }

    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_help();
        return;
    }

    if args.contains(&"--version".to_string()) || args.contains(&"-v".to_string()) {
        println!("eltr v{}", VERSION);
        return;
    }

    let type_check = args.contains(&"--check-types".to_string());
    let debug = args.contains(&"--debug".to_string()) || args.contains(&"-d".to_string());

    let subcommand = &args[1];
    let file = args.iter().skip(1).find(|a| !a.starts_with('-'));

    let result = match subcommand.as_str() {
        "lsp" => lsp::run_lsp(),
        "run" => {
            let file = args.iter().skip(2).find(|a| !a.starts_with('-'));
            match file {
                Some(path) => run_file_debug(path, type_check, debug),
                None => cmd_run_project(),
            }
        }
        "debug" => {
            let f = args.iter().skip(2).find(|a| !a.starts_with('-'));
            match f {
                Some(path) => {
                    let source = match fs::read_to_string(Path::new(path)) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("[IO] Could not read file '{}': {}", path, e);
                            std::process::exit(1);
                        }
                    };
                    let mut interpreter = Interpreter::new();
                    interpreter.jit_enabled = false;
                    interpreter.debugger = Some(Debugger::new());
                    if let Some(ref mut dbg) = interpreter.debugger {
                        dbg.mode = interpreter::debugger::DebugMode::StepInto;
                    }
                    interpreter.set_current_file(path);
                    run(&source, &mut interpreter, false)
                }
                None => {
                    eprintln!("error: 'debug' requires a file path");
                    std::process::exit(1);
                }
            }
        }
        "fmt" => {
            let check = args.contains(&"--check".to_string());
            let f = args.iter().skip(2).find(|a| !a.starts_with('-'));
            match f {
                Some(path) => do_fmt(path, check),
                None => {
                    eprintln!("error: 'fmt' requires a file path");
                    std::process::exit(1);
                }
            }
        }
        "test" => {
            let f = args.iter().skip(2).find(|a| !a.starts_with('-'));
            match f {
                Some(path) => do_test(path),
                None => {
                    eprintln!("error: 'test' requires a file path");
                    std::process::exit(1);
                }
            }
        }
        "init" => cmd_init(&args.iter().skip(2).cloned().collect::<Vec<_>>()),
        "build" => cmd_build_project(),
        "install" => cmd_install(&args.iter().skip(2).cloned().collect::<Vec<_>>()),
        _ => {
            match file {
                Some(path) => run_file_debug(path, type_check, debug),
                None => {
                    eprintln!("error: expected file path");
                    std::process::exit(1);
                }
            }
        }
    };

    if let Err(e) = result {
        let source = file.and_then(|p| fs::read_to_string(Path::new(p)).ok());
        if let Some(src) = source {
            eprintln!("{}", format_error(&src, &e));
        } else {
            eprintln!("[{}] {}", e.kind_label(), e.msg);
        }
        std::process::exit(1);
    }
}
