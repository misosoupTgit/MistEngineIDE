/// Mistral言語 意味解析・型チェッカー（修正版）
/// - 組み込み関数・モジュールの宣言漏れを修正
/// - 型名キーワード (str/int/float/bool) を組み込みとして認識

use std::collections::HashMap;
use crate::compiler::parser::{Stmt, Expr, TypeAnnot, BinOpKind};

#[derive(Debug, Clone, PartialEq)]
pub enum MistType {
    Int, Float, Str, Bool, List, Map, Null, Func, Any, Void,
}

impl From<&TypeAnnot> for MistType {
    fn from(t: &TypeAnnot) -> Self {
        match t {
            TypeAnnot::Int   => MistType::Int,
            TypeAnnot::Float => MistType::Float,
            TypeAnnot::Str   => MistType::Str,
            TypeAnnot::Bool  => MistType::Bool,
            TypeAnnot::List  => MistType::List,
            TypeAnnot::Map   => MistType::Map,
            TypeAnnot::Infer => MistType::Any,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisError { pub msg: String, pub line: usize }
impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AnalysisError at line {}: {}", self.line, self.msg)
    }
}

pub struct Scope { vars: HashMap<String, MistType> }
impl Scope {
    fn new() -> Self { Scope { vars: HashMap::new() } }
    fn declare(&mut self, name: &str, ty: MistType) { self.vars.insert(name.to_string(), ty); }
    fn lookup(&self, name: &str) -> Option<&MistType> { self.vars.get(name) }
}

pub struct Analyzer {
    scope_stack: Vec<Scope>,
    pub errors: Vec<AnalysisError>,
    pub warnings: Vec<String>,
    builtins: HashMap<String, MistType>,
}

impl Analyzer {
    pub fn new() -> Self {
        let mut builtins = HashMap::new();
        // 標準関数（Mistral識別子名）
        for f in &[
            "print","printf","debug",
            "str","int","float","bool",     // 型変換関数
            "len","typeof","range",
        ] {
            builtins.insert(f.to_string(), MistType::Func);
        }
        // モジュール（Any として扱い、メンバーアクセスでもエラーにしない）
        for m in &["math","draw","image","input","button","Color","Key","Controller"] {
            builtins.insert(m.to_string(), MistType::Any);
        }
        // ゲームコールバック (ready/update/draw/on_exit は自動定義されているとみなす)
        for f in &["ready","update","draw","on_exit"] {
            builtins.insert(f.to_string(), MistType::Func);
        }
        Analyzer { scope_stack: vec![Scope::new()], errors: Vec::new(), warnings: Vec::new(), builtins }
    }

    fn push_scope(&mut self) { self.scope_stack.push(Scope::new()); }
    fn pop_scope(&mut self) { self.scope_stack.pop(); }

    fn declare(&mut self, name: &str, ty: MistType) {
        if let Some(scope) = self.scope_stack.last_mut() { scope.declare(name, ty); }
    }

    fn lookup(&self, name: &str) -> Option<MistType> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(t) = scope.lookup(name) { return Some(t.clone()); }
        }
        self.builtins.get(name).cloned()
    }

    pub fn analyze(&mut self, stmts: &[Stmt]) {
        // 第1パス: 関数名を先に全部スコープ登録してから解析
        for stmt in stmts {
            if let Stmt::FuncDef { name, .. } = stmt {
                self.declare(name, MistType::Func);
            }
        }
        for stmt in stmts { self.check_stmt(stmt); }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Import(_) => {}

            Stmt::VarDecl { name, ty, init } => {
                let resolved_ty = if let Some(init_expr) = init {
                    let expr_ty = self.infer_expr(init_expr);
                    if matches!(ty, TypeAnnot::Infer) { expr_ty }
                    else {
                        let declared = MistType::from(ty);
                        // 型の不一致は警告にとどめる（Any は常に互換）
                        if declared != expr_ty && expr_ty != MistType::Any && declared != MistType::Any {
                            self.warnings.push(format!(
                                "Type hint mismatch for '{}': declared {:?} but initializer is {:?}", name, declared, expr_ty
                            ));
                        }
                        declared
                    }
                } else { MistType::from(ty) };
                self.declare(name, resolved_ty);
            }

            Stmt::FuncDef { name, params, ret, body } => {
                // 既に第1パスで登録済みだが念のため
                self.declare(name, MistType::Func);
                self.push_scope();
                for (pname, pty) in params { self.declare(pname, MistType::from(pty)); }
                // funcの内部でもネスト関数定義を先に登録
                for s in body.iter() {
                    if let Stmt::FuncDef { name: n, .. } = s { self.declare(n, MistType::Func); }
                }
                for s in body { self.check_stmt(s); }
                self.pop_scope();
                let _ = ret;
            }

            Stmt::Assign { target, op: _, value } => {
                // 未宣言変数への代入はエラーではなく無視（動的言語として緩く扱う）
                self.infer_expr(target);
                self.infer_expr(value);
            }

            Stmt::Increment { target, .. } => { self.infer_expr(target); }
            Stmt::Return(expr) => { if let Some(e) = expr { self.infer_expr(e); } }

            Stmt::If { cond, then_body, elseif_branches, else_body } => {
                self.infer_expr(cond);
                self.push_scope(); self.analyze(then_body); self.pop_scope();
                for (c, b) in elseif_branches {
                    self.infer_expr(c);
                    self.push_scope(); self.analyze(b); self.pop_scope();
                }
                if let Some(b) = else_body { self.push_scope(); self.analyze(b); self.pop_scope(); }
            }

            Stmt::Switch { expr, cases, default } => {
                self.infer_expr(expr);
                for (val, body) in cases {
                    self.infer_expr(val);
                    self.push_scope(); self.analyze(body); self.pop_scope();
                }
                if let Some(b) = default { self.push_scope(); self.analyze(b); self.pop_scope(); }
            }

            Stmt::While { cond, body } => {
                self.infer_expr(cond);
                self.push_scope(); self.analyze(body); self.pop_scope();
            }

            Stmt::ForIn { var, iter, body } => {
                self.infer_expr(iter);
                self.push_scope();
                self.declare(var, MistType::Any);
                self.analyze(body);
                self.pop_scope();
            }

            Stmt::ForRange { var, start, end, body } => {
                self.infer_expr(start); self.infer_expr(end);
                self.push_scope();
                self.declare(var, MistType::Int);
                self.analyze(body);
                self.pop_scope();
            }

            Stmt::Repeat { count, var, body } => {
                self.infer_expr(count);
                self.push_scope();
                if let Some(v) = var { self.declare(v, MistType::Int); }
                self.analyze(body);
                self.pop_scope();
            }

            Stmt::TryCatch { try_body, catch_var, catch_body } => {
                self.push_scope(); self.analyze(try_body); self.pop_scope();
                self.push_scope();
                self.declare(catch_var, MistType::Any);
                self.analyze(catch_body);
                self.pop_scope();
            }

            Stmt::Clone { expr, count, obj_var, idx_var, body } => {
                self.infer_expr(expr); self.infer_expr(count);
                self.push_scope();
                self.declare(obj_var, MistType::Any);
                self.declare(idx_var, MistType::Int);
                self.analyze(body);
                self.pop_scope();
            }

            Stmt::Expr(e) => { self.infer_expr(e); }
            Stmt::Break | Stmt::Continue => {}
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> MistType {
        match expr {
            Expr::IntLit(_)   => MistType::Int,
            Expr::FloatLit(_) => MistType::Float,
            Expr::StrLit(_)   => MistType::Str,
            Expr::BoolLit(_)  => MistType::Bool,
            Expr::Null        => MistType::Null,
            Expr::ListLit(_)  => MistType::List,

            Expr::Ident(name) => {
                // lookup できない場合はエラーではなく Any として続行
                self.lookup(name).unwrap_or(MistType::Any)
            }

            Expr::BinOp { op, lhs, rhs } => {
                let lt = self.infer_expr(lhs);
                let rt = self.infer_expr(rhs);
                match op {
                    BinOpKind::Add|BinOpKind::Sub|BinOpKind::Mul|BinOpKind::Div|BinOpKind::Mod|BinOpKind::Pow => {
                        if lt == MistType::Float || rt == MistType::Float { MistType::Float } else { MistType::Int }
                    }
                    BinOpKind::Eq|BinOpKind::NotEq|BinOpKind::Lt|BinOpKind::Gt|BinOpKind::LtEq|BinOpKind::GtEq|BinOpKind::And|BinOpKind::Or => MistType::Bool,
                }
            }

            Expr::UnaryOp { op: _, expr } => self.infer_expr(expr),

            Expr::Call { callee, args, kwargs } => {
                self.infer_expr(callee);
                for a in args { self.infer_expr(a); }
                for (_, v) in kwargs { self.infer_expr(v); }
                MistType::Any
            }

            Expr::Member { obj, .. } => {
                self.infer_expr(obj);
                MistType::Any
            }

            Expr::Index { obj, idx } => {
                self.infer_expr(obj); self.infer_expr(idx);
                MistType::Any
            }

            Expr::Lambda { params, ret, body } => {
                self.push_scope();
                for (p, t) in params { self.declare(p, MistType::from(t)); }
                self.analyze(body);
                self.pop_scope();
                let _ = ret;
                MistType::Func
            }
        }
    }
}
