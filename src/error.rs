use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceLoc {
    pub file: Option<String>,
    pub line: usize,
    pub col: usize,
}

#[allow(dead_code)]
impl SourceLoc {
    pub fn new(line: usize, col: usize) -> Self {
        SourceLoc { file: None, line, col }
    }
}

impl fmt::Display for SourceLoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{}:{}:{}", file, self.line, self.col)
        } else {
            write!(f, "{}:{}", self.line, self.col)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ErrorKind {
    Lex,
    Parse,
    TypeError,
    Runtime,
    IO,
    Import,
    Unwrap,
    Try,
}

impl ErrorKind {
    pub fn label(&self) -> &str {
        match self {
            ErrorKind::Lex => "Lexical error",
            ErrorKind::Parse => "Syntax error",
            ErrorKind::TypeError => "Type error",
            ErrorKind::Runtime => "Runtime error",
            ErrorKind::IO => "I/O error",
            ErrorKind::Import => "Import error",
            ErrorKind::Unwrap => "Unwrap error",
            ErrorKind::Try => "Propagation error",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Error {
    pub kind: ErrorKind,
    pub msg: String,
    pub loc: Option<SourceLoc>,
    pub source: Option<Box<Error>>,
}

impl Error {
    pub fn new(kind: ErrorKind, msg: impl Into<String>) -> Self {
        Error {
            kind,
            msg: msg.into(),
            loc: None,
            source: None,
        }
    }

    pub fn runtime(msg: impl Into<String>) -> Self {
        Error::new(ErrorKind::Runtime, msg)
    }

    pub fn kind_label(&self) -> &str {
        self.kind.label()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.kind_label())?;
        if let Some(loc) = &self.loc {
            write!(f, " at {}", loc)?;
        }
        write!(f, ": {}", self.msg)?;
        if let Some(source) = &self.source {
            write!(f, "\n  caused by: {}", source)?;
        }
        Ok(())
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::runtime(msg)
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::runtime(msg)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
