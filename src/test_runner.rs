use std::time::Instant;

use crate::ast::Stmt;
use crate::interpreter::Interpreter;
use crate::lexer::Lexer;
use crate::parser::Parser;

pub struct TestResults {
    pub total: usize,
    pub passed: usize,
    pub failed: Vec<(String, String)>,
    pub duration_ms: f64,
}

pub fn run_tests(source: &str, file_path: Option<&str>) -> Result<TestResults, String> {
    let tokens = Lexer::new(source).tokenize();
    let stmts = Parser::new(tokens).parse()?;

    let test_fns: Vec<String> = stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::Fn { name, params, .. } if name.starts_with("test_") && params.is_empty() => {
                Some(name.clone())
            }
            _ => None,
        })
        .collect();

    if test_fns.is_empty() {
        return Ok(TestResults {
            total: 0,
            passed: 0,
            failed: Vec::new(),
            duration_ms: 0.0,
        });
    }

    let start = Instant::now();

    let mut interpreter = Interpreter::new();
    if let Some(path) = file_path {
        interpreter.set_current_file(path);
    }
    interpreter.interpret(&stmts)?;

    let mut passed = 0;
    let mut failed = Vec::new();

    for fn_name in &test_fns {
        match interpreter.call_global_fn(fn_name, Vec::new()) {
            Ok(_) => passed += 1,
            Err(e) => failed.push((fn_name.clone(), e)),
        }
    }

    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

    Ok(TestResults {
        total: test_fns.len(),
        passed,
        failed,
        duration_ms,
    })
}

pub fn print_results(results: &TestResults) {
    println!(
        "\ntest result: {}. {} passed; {} failed out of {}; finished in {:.2}ms\n",
        if results.failed.is_empty() {
            "ok"
        } else {
            "FAILED"
        },
        results.passed,
        results.failed.len(),
        results.total,
        results.duration_ms,
    );

    for (name, msg) in &results.failed {
        println!("  FAIL {}\n    {}", name, msg);
    }
}
