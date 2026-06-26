use crate::ast::Stmt;

#[derive(Clone, Debug)]
pub struct Breakpoint {
    pub file: String,
    pub line: usize,
    pub enabled: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DebugMode {
    Running,
    StepInto,
    StepOver,
    StepOut,
    Paused,
}

pub struct Debugger {
    pub breakpoints: Vec<Breakpoint>,
    pub mode: DebugMode,
    pub call_depth: usize,
    pub step_depth: usize,
}

impl Debugger {
    pub fn new() -> Self {
        Debugger {
            breakpoints: Vec::new(),
            mode: DebugMode::Running,
            call_depth: 0,
            step_depth: 0,
        }
    }

    pub fn before_stmt(&mut self, stmt: &Stmt, file: &str) -> bool {
        let line = stmt.span().as_ref().map(|s| s.line).unwrap_or(0);
        if line == 0 {
            return false;
        }
        match self.mode {
            DebugMode::Running => {
                self.breakpoints.iter().any(|bp| bp.enabled && bp.line == line && bp.file == file)
            }
            DebugMode::StepInto => {
                self.mode = DebugMode::Paused;
                true
            }
            DebugMode::StepOver => {
                if self.call_depth <= self.step_depth {
                    self.mode = DebugMode::Paused;
                    true
                } else {
                    false
                }
            }
            DebugMode::StepOut => {
                if self.call_depth < self.step_depth {
                    self.mode = DebugMode::Paused;
                    true
                } else {
                    false
                }
            }
            DebugMode::Paused => false,
        }
    }

    pub fn before_call(&mut self) {
        self.call_depth += 1;
    }

    pub fn after_call(&mut self) {
        self.call_depth -= 1;
    }
}
