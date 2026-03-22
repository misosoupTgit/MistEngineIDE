/// Mistral言語 コードジェネレーター
/// ASTからRustコードを生成し、コンパイルして実行する

use std::fmt::Write as FmtWrite;
use crate::compiler::parser::{Stmt, Expr, BinOpKind, UnaryOpKind, AssignOp, TypeAnnot};

pub struct CodeGen {
    output: String,
    indent: usize,
}

impl CodeGen {
    pub fn new() -> Self {
        CodeGen { output: String::new(), indent: 0 }
    }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    fn emit(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn emitln(&mut self, s: &str) {
        let ind = self.indent_str();
        let _ = writeln!(self.output, "{}{}", ind, s);
    }

    pub fn generate(&mut self, stmts: &[Stmt]) -> String {
        // ランタイムヘッダー
        self.emit(RUNTIME_HEADER);
        self.emit("\nfn main() {\n");
        self.indent = 1;
        self.emit("    let mut _rt = MistRuntime::new();\n");
        self.emit("    _rt.run(|rt| {\n");
        self.indent = 2;
        for stmt in stmts {
            self.gen_stmt(stmt);
        }
        self.indent = 1;
        self.emit("    });\n");
        self.emit("}\n");
        self.output.clone()
    }

    fn gen_stmts(&mut self, stmts: &[Stmt]) {
        for s in stmts { self.gen_stmt(s); }
    }

    fn gen_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Import(path) => {
                self.emitln(&format!("// import \"{}\"", path));
            }

            Stmt::VarDecl { name, ty: _, init } => {
                if let Some(expr) = init {
                    let e = self.gen_expr(expr);
                    self.emitln(&format!("let mut {} = {};", name, e));
                } else {
                    self.emitln(&format!("let mut {}: Value = Value::Null;", name));
                }
            }

            Stmt::Assign { target, op, value } => {
                let t = self.gen_expr(target);
                let v = self.gen_expr(value);
                let op_str = match op {
                    AssignOp::Set => "=",
                    AssignOp::Add => "+=",
                    AssignOp::Sub => "-=",
                    AssignOp::Mul => "*=",
                    AssignOp::Div => "/=",
                };
                self.emitln(&format!("{} {} {};", t, op_str, v));
            }

            Stmt::Increment { target, delta } => {
                let t = self.gen_expr(target);
                if *delta > 0 {
                    self.emitln(&format!("{} += Value::Int(1);", t));
                } else {
                    self.emitln(&format!("{} -= Value::Int(1);", t));
                }
            }

            Stmt::FuncDef { name, params, ret: _, body } => {
                let params_str: Vec<String> = params.iter()
                    .map(|(p, _)| format!("{}: Value", p))
                    .collect();
                let sig = if params_str.is_empty() {
                    format!("fn {}() -> Value", name)
                } else {
                    format!("fn {}({}) -> Value", name, params_str.join(", "))
                };
                let ind = self.indent_str();
                self.emit(&format!("{}{} {{\n", ind, sig));
                self.indent += 1;
                self.gen_stmts(body);
                self.emitln("Value::Null");
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    let v = self.gen_expr(e);
                    self.emitln(&format!("return {};", v));
                } else {
                    self.emitln("return Value::Null;");
                }
            }

            Stmt::If { cond, then_body, elseif_branches, else_body } => {
                let c = self.gen_expr(cond);
                let ind = self.indent_str();
                self.emit(&format!("{}if {}.is_truthy() {{\n", ind, c));
                self.indent += 1;
                self.gen_stmts(then_body);
                self.indent -= 1;
                for (ec, eb) in elseif_branches {
                    let ec_str = self.gen_expr(ec);
                    let ind2 = self.indent_str();
                    self.emit(&format!("{}}} else if {}.is_truthy() {{\n", ind2, ec_str));
                    self.indent += 1;
                    self.gen_stmts(eb);
                    self.indent -= 1;
                }
                if let Some(eb) = else_body {
                    let ind2 = self.indent_str();
                    self.emit(&format!("{}}} else {{\n", ind2));
                    self.indent += 1;
                    self.gen_stmts(eb);
                    self.indent -= 1;
                }
                self.emitln("}");
            }

            Stmt::Switch { expr, cases, default } => {
                let e = self.gen_expr(expr);
                let ind = self.indent_str();
                self.emit(&format!("{}match {} {{\n", ind, e));
                self.indent += 1;
                for (val, body) in cases {
                    let v = self.gen_expr(val);
                    let ind2 = self.indent_str();
                    self.emit(&format!("{}x if x == {} => {{\n", ind2, v));
                    self.indent += 1;
                    self.gen_stmts(body);
                    self.indent -= 1;
                    self.emitln("}");
                }
                if let Some(db) = default {
                    self.emitln("_ => {");
                    self.indent += 1;
                    self.gen_stmts(db);
                    self.indent -= 1;
                    self.emitln("}");
                }
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::While { cond, body } => {
                let c = self.gen_expr(cond);
                let ind = self.indent_str();
                self.emit(&format!("{}while {}.is_truthy() {{\n", ind, c));
                self.indent += 1;
                self.gen_stmts(body);
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::ForIn { var, iter, body } => {
                let it = self.gen_expr(iter);
                let ind = self.indent_str();
                self.emit(&format!("{}for {} in {}.iter() {{\n", ind, var, it));
                self.indent += 1;
                self.gen_stmts(body);
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::ForRange { var, start, end, body } => {
                let s = self.gen_expr(start);
                let e = self.gen_expr(end);
                let ind = self.indent_str();
                self.emit(&format!("{}for {} in {}.as_int()..{}.as_int() {{\n", ind, var, s, e));
                self.indent += 1;
                self.gen_stmts(body);
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::Repeat { count, var, body } => {
                let c = self.gen_expr(count);
                let ind = self.indent_str();
                if let Some(v) = var {
                    self.emit(&format!("{}for {} in 0..{}.as_int() {{\n", ind, v, c));
                } else {
                    self.emit(&format!("{}for _ in 0..{}.as_int() {{\n", ind, c));
                }
                self.indent += 1;
                self.gen_stmts(body);
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::TryCatch { try_body, catch_var, catch_body } => {
                self.emitln("{ // try-catch");
                self.indent += 1;
                self.gen_stmts(try_body);
                self.indent -= 1;
                self.emitln("}");
                self.emitln(&format!("let {} = Value::Null; // catch", catch_var));
                self.indent += 1;
                self.gen_stmts(catch_body);
                self.indent -= 1;
            }

            Stmt::Clone { expr, count, obj_var, idx_var, body } => {
                let e = self.gen_expr(expr);
                let c = self.gen_expr(count);
                let ind = self.indent_str();
                self.emit(&format!("{}let _clone_base = {};\n", ind, e));
                self.emit(&format!("{}for {} in 0..{}.as_int() {{\n", ind, idx_var, c));
                self.indent += 1;
                self.emitln(&format!("let mut {} = _clone_base.clone();", obj_var));
                self.gen_stmts(body);
                self.indent -= 1;
                self.emitln("}");
            }

            Stmt::Expr(e) => {
                let v = self.gen_expr(e);
                self.emitln(&format!("{};", v));
            }

            Stmt::Break    => self.emitln("break;"),
            Stmt::Continue => self.emitln("continue;"),
        }
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::IntLit(n)   => format!("Value::Int({})", n),
            Expr::FloatLit(f) => format!("Value::Float({}_f64)", f),
            Expr::StrLit(s)   => format!("Value::Str({:?}.to_string())", s),
            Expr::BoolLit(b)  => format!("Value::Bool({})", b),
            Expr::Null        => "Value::Null".to_string(),
            Expr::ListLit(items) => {
                let elems: Vec<String> = items.iter().map(|e| self.gen_expr(e)).collect();
                format!("Value::List(vec![{}])", elems.join(", "))
            }
            Expr::Ident(name) => map_builtin_ident(name),

            Expr::BinOp { op, lhs, rhs } => {
                let l = self.gen_expr(lhs);
                let r = self.gen_expr(rhs);
                let op_str = match op {
                    BinOpKind::Add   => "+",
                    BinOpKind::Sub   => "-",
                    BinOpKind::Mul   => "*",
                    BinOpKind::Div   => "/",
                    BinOpKind::Mod   => "%",
                    BinOpKind::Pow   => "**",
                    BinOpKind::Eq    => "==",
                    BinOpKind::NotEq => "!=",
                    BinOpKind::Lt    => "<",
                    BinOpKind::Gt    => ">",
                    BinOpKind::LtEq  => "<=",
                    BinOpKind::GtEq  => ">=",
                    BinOpKind::And   => "&&",
                    BinOpKind::Or    => "||",
                };
                if matches!(op, BinOpKind::Pow) {
                    format!("{}.pow_val(&{})", l, r)
                } else if matches!(op, BinOpKind::And | BinOpKind::Or) {
                    format!("Value::Bool({}.is_truthy() {} {}.is_truthy())", l, op_str, r)
                } else if matches!(op, BinOpKind::Eq | BinOpKind::NotEq | BinOpKind::Lt
                    | BinOpKind::Gt | BinOpKind::LtEq | BinOpKind::GtEq) {
                    format!("Value::Bool(&{} {} &{})", l, op_str, r)
                } else {
                    format!("({} {} {})", l, op_str, r)
                }
            }

            Expr::UnaryOp { op, expr } => {
                let e = self.gen_expr(expr);
                match op {
                    UnaryOpKind::Neg => format!("(-{})", e),
                    UnaryOpKind::Not => format!("Value::Bool(!{}.is_truthy())", e),
                }
            }

            Expr::Call { callee, args, kwargs } => {
                let mut arg_strs: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                match callee.as_ref() {
                    // draw.xxx(...)  → mist_draw_xxx(...)
                    Expr::Member { obj, field } if matches!(obj.as_ref(), Expr::Ident(n) if n == "draw") => {
                        let kw: Vec<String> = kwargs.iter()
                            .map(|(k,v)| format!("{}: {}", k, self.gen_expr(v))).collect();
                        if !kw.is_empty() { arg_strs.push(format!("/* {} */", kw.join(", "))); }
                        format!("mist_draw_{}({})", field, arg_strs.join(", "))
                    }
                    // math.xxx(...)  → mist_math_xxx(...)
                    Expr::Member { obj, field } if matches!(obj.as_ref(), Expr::Ident(n) if n == "math") => {
                        format!("mist_math_{}({})", field, arg_strs.join(", "))
                    }
                    // obj.method(...)  → obj.method_val(...)
                    Expr::Member { obj, field } => {
                        let o = self.gen_expr(obj);
                        if !kwargs.is_empty() {
                            let kw: Vec<String> = kwargs.iter()
                                .map(|(k,v)| format!("(\"{}\", {})", k, self.gen_expr(v))).collect();
                            arg_strs.push(format!("&[{}]", kw.join(", ")));
                        }
                        format!("{}.mist_{}({})", o, field, arg_strs.join(", "))
                    }
                    // 通常の関数呼び出し
                    _ => {
                        let c = self.gen_expr(callee);
                        if !kwargs.is_empty() {
                            let kw: Vec<String> = kwargs.iter()
                                .map(|(k,v)| format!("(\"{}\", {})", k, self.gen_expr(v))).collect();
                            arg_strs.push(format!("&[{}]", kw.join(", ")));
                        }
                        format!("{}({})", c, arg_strs.join(", "))
                    }
                }
            }

            Expr::Member { obj, field } => {
                let o = self.gen_expr(obj);
                format!("{}.field_{}", o, field)
            }

            Expr::Index { obj, idx } => {
                let o = self.gen_expr(obj);
                let i = self.gen_expr(idx);
                format!("{}[{}.as_int() as usize]", o, i)
            }

            Expr::Lambda { params, ret: _, body } => {
                let params_str: Vec<String> = params.iter()
                    .map(|(p, _)| format!("{}: Value", p))
                    .collect();
                let mut inner = CodeGen::new();
                inner.indent = 1;
                for s in body { inner.gen_stmt(s); }
                inner.emitln("Value::Null");
                format!("|{}| {{\n{}}}", params_str.join(", "), inner.output)
            }
        }
    }
}

// Rustランタイムテンプレート（生成コードに埋め込む）

/// Mistral識別子名 → 生成Rustコード識別子名 のマッピング
fn map_builtin_ident(name: &str) -> String {
    match name {
        "str"   => "str_fn".to_string(),
        "int"   => "int_fn".to_string(),
        "float" => "float_fn".to_string(),
        "bool"  => "bool_fn".to_string(),
        other   => other.to_string(),
    }
}

const RUNTIME_HEADER: &str = r#"
// === MistEngine Runtime ===
// 自動生成されたコード

use std::collections::HashMap;
use std::fmt;
use std::ops::{Add, Sub, Mul, Div, Rem};

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Null,
}

impl Value {
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Null => false,
            _ => true,
        }
    }

    pub fn as_int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => if *b { 1 } else { 0 },
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            Value::Float(f) => *f,
            Value::Int(n) => *n as f64,
            _ => 0.0,
        }
    }

    pub fn pow_val(&self, rhs: &Value) -> Value {
        let b = self.as_float();
        let e = rhs.as_float();
        Value::Float(b.powf(e))
    }

    pub fn iter(&self) -> std::slice::Iter<Value> {
        if let Value::List(v) = self { v.iter() }
        else { panic!("Not iterable") }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(n)   => write!(f, "{}", n),
            Value::Float(v) => write!(f, "{}", v),
            Value::Str(s)   => write!(f, "{}", s),
            Value::Bool(b)  => write!(f, "{}", b),
            Value::Null     => write!(f, "null"),
            Value::List(v)  => {
                write!(f, "[")?;
                for (i, item) in v.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(_)   => write!(f, "{{...}}"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_float().partial_cmp(&other.as_float())
    }
}

impl Add for Value {
    type Output = Value;
    fn add(self, rhs: Value) -> Value {
        match (&self, &rhs) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
            (Value::Str(a), Value::Str(b)) => Value::Str(format!("{}{}", a, b)),
            _ => Value::Float(self.as_float() + rhs.as_float()),
        }
    }
}

impl Sub for Value {
    type Output = Value;
    fn sub(self, rhs: Value) -> Value {
        match (&self, &rhs) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
            _ => Value::Float(self.as_float() - rhs.as_float()),
        }
    }
}

impl Mul for Value {
    type Output = Value;
    fn mul(self, rhs: Value) -> Value {
        match (&self, &rhs) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
            _ => Value::Float(self.as_float() * rhs.as_float()),
        }
    }
}

impl Div for Value {
    type Output = Value;
    fn div(self, rhs: Value) -> Value {
        match (&self, &rhs) {
            (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a / b),
            _ => Value::Float(self.as_float() / rhs.as_float()),
        }
    }
}

impl Rem for Value {
    type Output = Value;
    fn rem(self, rhs: Value) -> Value {
        match (&self, &rhs) {
            (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a % b),
            _ => Value::Float(self.as_float() % rhs.as_float()),
        }
    }
}

// 標準関数
pub fn print(v: Value) -> Value { println!("{}", v); Value::Null }
pub fn printf(fmt_str: Value, kwargs: &[(&str, Value)]) -> Value {
    let mut s = fmt_str.to_string();
    for (k, v) in kwargs { s = s.replace(&format!("{{{}}}", k), &format!("{}", v)); }
    println!("{}", s);
    Value::Null
}
pub fn debug(v: Value) -> Value {
    #[cfg(debug_assertions)]
    eprintln!("\x1b[36m[debug] {}\x1b[0m", v);
    Value::Null
}
pub fn str_fn(v: Value) -> Value { Value::Str(format!("{}", v)) }
pub fn int_fn(v: Value) -> Value { Value::Int(v.as_int()) }
pub fn float_fn(v: Value) -> Value { Value::Float(v.as_float()) }
pub fn bool_fn(v: Value) -> Value { Value::Bool(v.is_truthy()) }
pub fn len(v: Value) -> Value {
    match &v {
        Value::List(l) => Value::Int(l.len() as i64),
        Value::Str(s)  => Value::Int(s.len() as i64),
        _ => Value::Int(0),
    }
}
pub fn typeof_fn(v: Value) -> Value {
    let t = match &v {
        Value::Int(_)   => "int",
        Value::Float(_) => "float",
        Value::Str(_)   => "str",
        Value::Bool(_)  => "bool",
        Value::List(_)  => "list",
        Value::Map(_)   => "map",
        Value::Null     => "null",
    };
    Value::Str(t.to_string())
}

// math モジュール関数スタブ
pub fn mist_math_sin(v: Value) -> Value { Value::Float(v.as_float().sin()) }
pub fn mist_math_cos(v: Value) -> Value { Value::Float(v.as_float().cos()) }
pub fn mist_math_sqrt(v: Value) -> Value { Value::Float(v.as_float().sqrt()) }
pub fn mist_math_abs(v: Value) -> Value { Value::Float(v.as_float().abs()) }
pub fn mist_math_floor(v: Value) -> Value { Value::Float(v.as_float().floor()) }
pub fn mist_math_ceil(v: Value) -> Value { Value::Float(v.as_float().ceil()) }
pub fn mist_math_round(v: Value) -> Value { Value::Float(v.as_float().round()) }
pub fn mist_math_max(a: Value, b: Value) -> Value { if a.as_float() >= b.as_float() { a } else { b } }
pub fn mist_math_min(a: Value, b: Value) -> Value { if a.as_float() <= b.as_float() { a } else { b } }
pub fn mist_math_pow(b: Value, e: Value) -> Value { Value::Float(b.as_float().powf(e.as_float())) }
pub fn mist_math_rand() -> Value { Value::Float(0.5) } // 簡易スタブ
pub fn mist_math_rand_int(lo: Value, hi: Value) -> Value { Value::Int((lo.as_int() + hi.as_int()) / 2) }
pub fn mist_math_clamp(v: Value, lo: Value, hi: Value) -> Value {
    Value::Float(v.as_float().max(lo.as_float()).min(hi.as_float()))
}
pub fn mist_math_lerp(a: Value, b: Value, t: Value) -> Value {
    Value::Float(a.as_float() + (b.as_float() - a.as_float()) * t.as_float())
}

// draw モジュール関数スタブ（IDE実行時は何もしない）
pub fn mist_draw_circle(_x: Value, _y: Value, _r: Value) -> Value { Value::Null }
pub fn mist_draw_rect(_x: Value, _y: Value, _w: Value, _h: Value) -> Value { Value::Null }
pub fn mist_draw_square(_x: Value, _y: Value, _s: Value) -> Value { Value::Null }
pub fn mist_draw_line(_x1: Value, _y1: Value, _x2: Value, _y2: Value) -> Value { Value::Null }
pub fn mist_draw_triangle(_x: Value, _y: Value, _s: Value) -> Value { Value::Null }
pub fn mist_draw_polygon(_x: Value, _y: Value, _s: Value) -> Value { Value::Null }

pub struct MistRuntime;
impl MistRuntime {
    pub fn new() -> Self { MistRuntime }
    pub fn run<F: FnOnce(&mut MistRuntime)>(&mut self, f: F) { f(self); }
}

"#;
