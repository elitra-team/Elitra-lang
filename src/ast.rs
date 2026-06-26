use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    String,
    Bool,
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Fn(Vec<Type>, Box<Type>),
    Result(Box<Type>, Box<Type>),
    Option(Box<Type>),
    Nil,
    Any,
    Tuple(Vec<Type>),
    Range,
    SelfType,
    Instance(String),
    Generic(String),
    TraitObject(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub line: usize,
    pub col: usize,
}

pub type HasSpan = Option<SourceSpan>;

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        span: HasSpan,
        pub_flag: bool,
        name: String,
        type_ann: Option<Type>,
        value: Box<Expr>,
    },
    Struct {
        span: HasSpan,
        pub_flag: bool,
        name: String,
        fields: Vec<(String, Option<Type>)>,
    },
    Enum {
        span: HasSpan,
        pub_flag: bool,
        name: String,
        variants: Vec<EnumVariant>,
    },
    Fn {
        span: HasSpan,
        pub_flag: bool,
        name: String,
        generic_params: Vec<String>,
        params: Vec<(String, Option<Type>, Option<Expr>)>,
        return_type: Option<Type>,
        body: Vec<Stmt>,
        is_async: bool,
    },
    Macro {
        span: HasSpan,
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    If {
        span: HasSpan,
        condition: Box<Expr>,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        span: HasSpan,
        condition: Box<Expr>,
        body: Vec<Stmt>,
    },
    For {
        span: HasSpan,
        var: String,
        iterable: Box<Expr>,
        body: Vec<Stmt>,
    },
    Break {
        span: HasSpan,
    },
    Continue {
        span: HasSpan,
    },
    Match {
        span: HasSpan,
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    Try {
        span: HasSpan,
        body: Vec<Stmt>,
        catch_var: String,
        catch_body: Vec<Stmt>,
    },
    Return {
        span: HasSpan,
        value: Box<Expr>,
    },
    Print {
        span: HasSpan,
        value: Box<Expr>,
        newline: bool,
    },
    Import {
        span: HasSpan,
        pub_flag: bool,
        path: String,
        alias: Option<String>,
    },
    Class {
        span: HasSpan,
        pub_flag: bool,
        name: String,
        extends: Option<String>,
        methods: Vec<ClassMethod>,
    },
    DoWhile {
        span: HasSpan,
        body: Vec<Stmt>,
        condition: Box<Expr>,
    },
    Destructure {
        span: HasSpan,
        pub_flag: bool,
        target: DestructureTarget,
        value: Box<Expr>,
    },
    Yield {
        span: HasSpan,
        value: Box<Expr>,
    },
    Throw {
        span: HasSpan,
        value: Box<Expr>,
    },
    Trait {
        span: HasSpan,
        pub_flag: bool,
        name: String,
        methods: Vec<TraitMethod>,
    },
    Impl {
        span: HasSpan,
        trait_name: String,
        type_name: String,
        methods: Vec<TraitMethodImpl>,
    },
    Expr {
        span: HasSpan,
        expr: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DestructureTarget {
    List(Vec<DestructureItem>),
    Struct(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DestructureItem {
    Name(String),
    Rest(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<(String, Option<Type>, Option<Expr>)>,
    pub return_type: Option<Type>,
    pub body: Option<Vec<Stmt>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethodImpl {
    pub name: String,
    pub params: Vec<(String, Option<Type>, Option<Expr>)>,
    pub return_type: Option<Type>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassMethod {
    pub name: String,
    pub params: Vec<(String, Option<Type>, Option<Expr>)>,
    pub body: Vec<Stmt>,
}

impl Stmt {
    pub fn span(&self) -> HasSpan {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Struct { span, .. }
            | Stmt::Enum { span, .. }
            | Stmt::Fn { span, .. }
            | Stmt::Macro { span, .. }
            | Stmt::If { span, .. }
            | Stmt::While { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Break { span, .. }
            | Stmt::Continue { span, .. }
            | Stmt::Match { span, .. }
            | Stmt::Try { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Print { span, .. }
            | Stmt::Import { span, .. }
            | Stmt::Class { span, .. }
            | Stmt::DoWhile { span, .. }
            | Stmt::Destructure { span, .. }
            | Stmt::Yield { span, .. }
            | Stmt::Throw { span, .. }
            | Stmt::Trait { span, .. }
            | Stmt::Impl { span, .. }
            | Stmt::Expr { span, .. } => span.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern {
    Literal(Expr),
    Wildcard,
    Binding(String),
    Destructure(String, Vec<String>),
    Or(Vec<MatchPattern>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Nil,
    Variable(String),
    Fn {
        generic_params: Vec<String>,
        params: Vec<(String, Option<Type>, Option<Expr>)>,
        body: Vec<Stmt>,
    },
    List(Vec<Expr>),
    Set(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Tuple(Vec<Expr>),
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        object: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Spread(Box<Expr>),
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
    },
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOpKind,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOpKind,
        right: Box<Expr>,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    Assignment {
        name: String,
        value: Box<Expr>,
    },
    CompoundAssign {
        name: String,
        op: BinaryOpKind,
        value: Box<Expr>,
    },
    Grouping(Box<Expr>),
    StringInterp(Vec<Expr>),
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    Await {
        value: Box<Expr>,
    },
    FieldAssign {
        object: Box<Expr>,
        field: String,
        value: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Try {
        expr: Box<Expr>,
    },
    Super,
    ListComp {
        expr: Box<Expr>,
        clauses: Vec<CompClause>,
    },
    SetComp {
        expr: Box<Expr>,
        clauses: Vec<CompClause>,
    },
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        clauses: Vec<CompClause>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompClause {
    pub var: String,
    pub iterable: Box<Expr>,
    pub conditions: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOpKind {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    In,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOpKind {
    Negate,
    Not,
    BitNot,
}

impl fmt::Display for BinaryOpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOpKind::Add => write!(f, "+"),
            BinaryOpKind::Subtract => write!(f, "-"),
            BinaryOpKind::Multiply => write!(f, "*"),
            BinaryOpKind::Divide => write!(f, "/"),
            BinaryOpKind::Modulo => write!(f, "%"),
            BinaryOpKind::Equal => write!(f, "=="),
            BinaryOpKind::NotEqual => write!(f, "!="),
            BinaryOpKind::Less => write!(f, "<"),
            BinaryOpKind::LessEqual => write!(f, "<="),
            BinaryOpKind::Greater => write!(f, ">"),
            BinaryOpKind::GreaterEqual => write!(f, ">="),
            BinaryOpKind::And => write!(f, "&&"),
            BinaryOpKind::Or => write!(f, "||"),
            BinaryOpKind::In => write!(f, "in"),
            BinaryOpKind::BitAnd => write!(f, "&"),
            BinaryOpKind::BitOr => write!(f, "|"),
            BinaryOpKind::BitXor => write!(f, "^"),
            BinaryOpKind::ShiftLeft => write!(f, "<<"),
            BinaryOpKind::ShiftRight => write!(f, ">>"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<(String, Option<Type>)>,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::String => write!(f, "string"),
            Type::Bool => write!(f, "bool"),
            Type::List(inner) => write!(f, "list<{}>", inner),
            Type::Dict(k, v) => write!(f, "dict<{}, {}>", k, v),
            Type::Fn(params, ret) => {
                let ps: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                write!(f, "fn({}) -> {}", ps.join(", "), ret)
            }
            Type::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            Type::Option(inner) => write!(f, "Option<{}>", inner),
            Type::Tuple(ts) => {
                let ps: Vec<String> = ts.iter().map(|t| t.to_string()).collect();
                write!(f, "({})", ps.join(", "))
            }
            Type::Range => write!(f, "range"),
            Type::Nil => write!(f, "none"),
            Type::Any => write!(f, "any"),
            Type::SelfType => write!(f, "Self"),
            Type::Instance(name) => write!(f, "{}", name),
            Type::Generic(name) => write!(f, "{}", name),
            Type::TraitObject(name) => write!(f, "impl {}", name),
        }
    }
}
