/// Mistral言語 Parser / AST
/// GDScript構文をベースとした構文解析器

use crate::compiler::lexer::{Token, TokenKind};

// ==============================
// AST ノード定義
// ==============================

#[derive(Debug, Clone)]
pub enum TypeAnnot {
    Int,
    Float,
    Str,
    Bool,
    List,
    Map,
    Infer, // 型推論
}

#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    BoolLit(bool),
    Null,
    Ident(String),

    // binary op
    BinOp {
        op: BinOpKind,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    // unary op
    UnaryOp {
        op: UnaryOpKind,
        expr: Box<Expr>,
    },
    // 関数呼び出し（通常 + キーワード引数）
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        kwargs: Vec<(String, Expr)>,
    },
    // メンバーアクセス: a.b
    Member {
        obj: Box<Expr>,
        field: String,
    },
    // インデックス: a[i]
    Index {
        obj: Box<Expr>,
        idx: Box<Expr>,
    },
    // リストリテラル
    ListLit(Vec<Expr>),
    // 無名関数: func() { ... }
    Lambda {
        params: Vec<(String, TypeAnnot)>,
        ret: TypeAnnot,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone)]
pub enum BinOpKind {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, NotEq, Lt, Gt, LtEq, GtEq,
    And, Or,
}

#[derive(Debug, Clone)]
pub enum UnaryOpKind {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    // let x: int = expr
    VarDecl {
        name: String,
        ty: TypeAnnot,
        init: Option<Expr>,
    },
    // 代入・複合代入
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    // ++ / --
    Increment { target: Expr, delta: i64 },

    // 関数定義
    FuncDef {
        name: String,
        params: Vec<(String, TypeAnnot)>,
        ret: TypeAnnot,
        body: Vec<Stmt>,
    },

    // return
    Return(Option<Expr>),

    // if / ifelse / else
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        elseif_branches: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },

    // switch
    Switch {
        expr: Expr,
        cases: Vec<(Expr, Vec<Stmt>)>,
        default: Option<Vec<Stmt>>,
    },

    // while
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },

    // for item in list
    ForIn {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },

    // for i in range(s, e)
    ForRange {
        var: String,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
    },

    // repeat(n) { } / repeat(n, x) { }
    Repeat {
        count: Expr,
        var: Option<String>,
        body: Vec<Stmt>,
    },

    // try { } catch e { }
    TryCatch {
        try_body: Vec<Stmt>,
        catch_var: String,
        catch_body: Vec<Stmt>,
    },

    // import "file.mist"
    Import(String),

    // clone draw.circle(...) * n as c, i { }
    Clone {
        expr: Expr,
        count: Expr,
        obj_var: String,
        idx_var: String,
        body: Vec<Stmt>,
    },

    // 式文
    Expr(Expr),

    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
}

// ==============================
// Parser
// ==============================

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        // Newlineトークンを除去してから解析
        let tokens: Vec<Token> = tokens.into_iter()
            .filter(|t| !matches!(t.kind, TokenKind::Newline))
            .collect();
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &TokenKind {
        self.tokens.get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn peek_token(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(self.tokens.last().unwrap())
    }

    fn advance(&mut self) -> &Token {
        let t = &self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<&Token, ParseError> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(kind) {
            Ok(self.advance())
        } else {
            let tok = self.peek_token().clone();
            Err(ParseError {
                msg: format!("Expected {:?} but got {:?}", kind, tok.kind),
                line: tok.line,
                col: tok.col,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if let TokenKind::Ident(s) = self.peek().clone() {
            let name = s.clone();
            self.advance();
            Ok(name)
        } else {
            let tok = self.peek_token().clone();
            Err(ParseError {
                msg: format!("Expected identifier, got {:?}", tok.kind),
                line: tok.line,
                col: tok.col,
            })
        }
    }

    fn expect_str_lit(&mut self) -> Result<String, ParseError> {
        if let TokenKind::StrLit(s) = self.peek().clone() {
            let v = s.clone();
            self.advance();
            Ok(v)
        } else {
            let tok = self.peek_token().clone();
            Err(ParseError {
                msg: format!("Expected string literal, got {:?}", tok.kind),
                line: tok.line,
                col: tok.col,
            })
        }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub fn parse(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(stmts)
    }

    fn parse_type_annot(&mut self) -> TypeAnnot {
        match self.peek() {
            TokenKind::TypeInt   => { self.advance(); TypeAnnot::Int }
            TokenKind::TypeFloat => { self.advance(); TypeAnnot::Float }
            TokenKind::TypeStr   => { self.advance(); TypeAnnot::Str }
            TokenKind::TypeBool  => { self.advance(); TypeAnnot::Bool }
            TokenKind::TypeList  => { self.advance(); TypeAnnot::List }
            TokenKind::TypeMap   => { self.advance(); TypeAnnot::Map }
            _ => TypeAnnot::Infer,
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.peek().clone() {
            TokenKind::Import => {
                self.advance();
                let path = self.expect_str_lit()?;
                Ok(Stmt::Import(path))
            }
            TokenKind::Let => self.parse_var_decl(),
            TokenKind::Func => self.parse_func_def(),
            TokenKind::Return => {
                self.advance();
                if self.check(&TokenKind::RBrace) || self.check(&TokenKind::Eof) {
                    Ok(Stmt::Return(None))
                } else {
                    Ok(Stmt::Return(Some(self.parse_expr()?)))
                }
            }
            TokenKind::If => self.parse_if(),
            TokenKind::Switch => self.parse_switch(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Repeat => self.parse_repeat(),
            TokenKind::Try => self.parse_try_catch(),
            TokenKind::Clone => self.parse_clone(),
            TokenKind::Break => { self.advance(); Ok(Stmt::Break) }
            TokenKind::Continue => { self.advance(); Ok(Stmt::Continue) }
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // let
        let name = self.expect_ident()?;
        let ty = if self.eat(&TokenKind::Colon) {
            self.parse_type_annot()
        } else {
            TypeAnnot::Infer
        };
        let init = if self.eat(&TokenKind::Assign) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        Ok(Stmt::VarDecl { name, ty, init })
    }

    fn parse_func_def(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // func
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let pname = self.expect_ident()?;
            let pty = if self.eat(&TokenKind::Colon) {
                self.parse_type_annot()
            } else {
                TypeAnnot::Infer
            };
            // デフォルト引数（= expr）は現時点でスキップ
            if self.eat(&TokenKind::Assign) {
                self.parse_expr()?; // consume default value
            }
            params.push((pname, pty));
            if !self.eat(&TokenKind::Comma) { break; }
        }
        self.expect(&TokenKind::RParen)?;
        let ret = if self.eat(&TokenKind::Arrow) {
            self.parse_type_annot()
        } else {
            TypeAnnot::Infer
        };
        let body = self.parse_block()?;
        Ok(Stmt::FuncDef { name, params, ret, body })
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        let then_body = self.parse_block()?;
        let mut elseif_branches = Vec::new();
        let mut else_body = None;
        loop {
            if self.check(&TokenKind::Ifelse) {
                self.advance();
                let c = self.parse_expr()?;
                let b = self.parse_block()?;
                elseif_branches.push((c, b));
            } else if self.check(&TokenKind::Else) {
                self.advance();
                else_body = Some(self.parse_block()?);
                break;
            } else {
                break;
            }
        }
        Ok(Stmt::If { cond, then_body, elseif_branches, else_body })
    }

    fn parse_switch(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // switch
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::LBrace)?;
        let mut cases = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Case) {
                self.advance();
                let val = self.parse_expr()?;
                self.expect(&TokenKind::Colon)?;
                let body = self.parse_case_body()?;
                cases.push((val, body));
            } else if self.check(&TokenKind::Default) {
                self.advance();
                self.expect(&TokenKind::Colon)?;
                default = Some(self.parse_case_body()?);
            } else {
                break;
            }
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Stmt::Switch { expr, cases, default })
    }

    fn parse_case_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        // case body はブロックではなく、次のcase/default/}まで
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::Case)
            && !self.check(&TokenKind::Default)
            && !self.check(&TokenKind::RBrace)
            && !self.check(&TokenKind::Eof)
        {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // while
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // for
        let var = self.expect_ident()?;
        self.expect(&TokenKind::In)?;
        // range(s, e) か イテレータか判断
        if self.check(&TokenKind::Range) {
            self.advance();
            self.expect(&TokenKind::LParen)?;
            let start = self.parse_expr()?;
            self.expect(&TokenKind::Comma)?;
            let end = self.parse_expr()?;
            self.expect(&TokenKind::RParen)?;
            let body = self.parse_block()?;
            Ok(Stmt::ForRange { var, start, end, body })
        } else {
            let iter = self.parse_expr()?;
            let body = self.parse_block()?;
            Ok(Stmt::ForIn { var, iter, body })
        }
    }

    fn parse_repeat(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // repeat
        self.expect(&TokenKind::LParen)?;
        let count = self.parse_expr()?;
        let var = if self.eat(&TokenKind::Comma) {
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect(&TokenKind::RParen)?;
        let body = self.parse_block()?;
        Ok(Stmt::Repeat { count, var, body })
    }

    fn parse_try_catch(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // try
        let try_body = self.parse_block()?;
        self.expect(&TokenKind::Catch)?;
        let catch_var = self.expect_ident()?;
        let catch_body = self.parse_block()?;
        Ok(Stmt::TryCatch { try_body, catch_var, catch_body })
    }

    fn parse_clone(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // clone
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::Star)?;
        let count = self.parse_expr()?;
        self.expect(&TokenKind::As)?;
        let obj_var = self.expect_ident()?;
        self.expect(&TokenKind::Comma)?;
        let idx_var = self.expect_ident()?;
        let body = self.parse_block()?;
        Ok(Stmt::Clone { expr, count, obj_var, idx_var, body })
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;
        // 後置 ++ / --
        if self.check(&TokenKind::PlusPlus) {
            self.advance();
            return Ok(Stmt::Increment { target: expr, delta: 1 });
        }
        if self.check(&TokenKind::MinusMinus) {
            self.advance();
            return Ok(Stmt::Increment { target: expr, delta: -1 });
        }
        // 代入
        let op = match self.peek() {
            TokenKind::Assign      => { self.advance(); AssignOp::Set }
            TokenKind::PlusAssign  => { self.advance(); AssignOp::Add }
            TokenKind::MinusAssign => { self.advance(); AssignOp::Sub }
            TokenKind::StarAssign  => { self.advance(); AssignOp::Mul }
            TokenKind::SlashAssign => { self.advance(); AssignOp::Div }
            _ => return Ok(Stmt::Expr(expr)),
        };
        let value = self.parse_expr()?;
        Ok(Stmt::Assign { target: expr, op, value })
    }

    // ==============================
    // 式解析（演算子優先度）
    // ==============================

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and()?;
        while self.check(&TokenKind::Or) {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Expr::BinOp { op: BinOpKind::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_cmp()?;
        while self.check(&TokenKind::And) {
            self.advance();
            let rhs = self.parse_cmp()?;
            lhs = Expr::BinOp { op: BinOpKind::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek() {
                TokenKind::Eq    => BinOpKind::Eq,
                TokenKind::NotEq => BinOpKind::NotEq,
                TokenKind::Lt    => BinOpKind::Lt,
                TokenKind::Gt    => BinOpKind::Gt,
                TokenKind::LtEq  => BinOpKind::LtEq,
                TokenKind::GtEq  => BinOpKind::GtEq,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_add()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus  => BinOpKind::Add,
                TokenKind::Minus => BinOpKind::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_mul()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_pow()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star    => BinOpKind::Mul,
                TokenKind::Slash   => BinOpKind::Div,
                TokenKind::Percent => BinOpKind::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_pow()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_pow(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_unary()?;
        if self.check(&TokenKind::StarStar) {
            self.advance();
            let exp = self.parse_unary()?;
            Ok(Expr::BinOp { op: BinOpKind::Pow, lhs: Box::new(base), rhs: Box::new(exp) })
        } else {
            Ok(base)
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.check(&TokenKind::Minus) {
            self.advance();
            let e = self.parse_postfix()?;
            return Ok(Expr::UnaryOp { op: UnaryOpKind::Neg, expr: Box::new(e) });
        }
        if self.check(&TokenKind::Not) {
            self.advance();
            let e = self.parse_postfix()?;
            return Ok(Expr::UnaryOp { op: UnaryOpKind::Not, expr: Box::new(e) });
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let field = self.expect_ident()?;
                // メソッド呼び出し？
                if self.check(&TokenKind::LParen) {
                    let (args, kwargs) = self.parse_call_args()?;
                    expr = Expr::Call {
                        callee: Box::new(Expr::Member { obj: Box::new(expr), field }),
                        args,
                        kwargs,
                    };
                } else {
                    expr = Expr::Member { obj: Box::new(expr), field };
                }
            } else if self.check(&TokenKind::LBracket) {
                self.advance();
                let idx = self.parse_expr()?;
                self.expect(&TokenKind::RBracket)?;
                expr = Expr::Index { obj: Box::new(expr), idx: Box::new(idx) };
            } else if self.check(&TokenKind::LParen) {
                let (args, kwargs) = self.parse_call_args()?;
                expr = Expr::Call { callee: Box::new(expr), args, kwargs };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_call_args(&mut self) -> Result<(Vec<Expr>, Vec<(String, Expr)>), ParseError> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            // キーワード引数: name=expr
            if let TokenKind::Ident(name) = self.peek().clone() {
                // 先読み: name= ?
                if self.tokens.get(self.pos + 1).map(|t| matches!(t.kind, TokenKind::Assign)).unwrap_or(false) {
                    let kname = name.clone();
                    self.advance(); // name
                    self.advance(); // =
                    let val = self.parse_expr()?;
                    kwargs.push((kname, val));
                    if !self.eat(&TokenKind::Comma) { break; }
                    continue;
                }
            }
            args.push(self.parse_expr()?);
            if !self.eat(&TokenKind::Comma) { break; }
        }
        self.expect(&TokenKind::RParen)?;
        Ok((args, kwargs))
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            TokenKind::IntLit(n)  => { self.advance(); Ok(Expr::IntLit(n)) }
            TokenKind::FloatLit(f) => { self.advance(); Ok(Expr::FloatLit(f)) }
            TokenKind::StrLit(s)  => { self.advance(); Ok(Expr::StrLit(s)) }
            TokenKind::BoolLit(b) => { self.advance(); Ok(Expr::BoolLit(b)) }
            TokenKind::Null       => { self.advance(); Ok(Expr::Null) }
            TokenKind::Ident(s)   => { self.advance(); Ok(Expr::Ident(s)) }
            TokenKind::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(e)
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                    items.push(self.parse_expr()?);
                    if !self.eat(&TokenKind::Comma) { break; }
                }
                self.expect(&TokenKind::RBracket)?;
                Ok(Expr::ListLit(items))
            }
            // 無名関数: func() { }
            TokenKind::Func => {
                self.advance();
                self.expect(&TokenKind::LParen)?;
                let mut params = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                    let p = self.expect_ident()?;
                    let t = if self.eat(&TokenKind::Colon) { self.parse_type_annot() } else { TypeAnnot::Infer };
                    params.push((p, t));
                    if !self.eat(&TokenKind::Comma) { break; }
                }
                self.expect(&TokenKind::RParen)?;
                let ret = if self.eat(&TokenKind::Arrow) { self.parse_type_annot() } else { TypeAnnot::Infer };
                let body = self.parse_block()?;
                Ok(Expr::Lambda { params, ret, body })
            }
            // 型名（Color等）も識別子として扱う
            TokenKind::TypeInt   => { self.advance(); Ok(Expr::Ident("int".to_string())) }
            TokenKind::TypeFloat => { self.advance(); Ok(Expr::Ident("float".to_string())) }
            TokenKind::TypeStr   => { self.advance(); Ok(Expr::Ident("str".to_string())) }
            TokenKind::TypeBool  => { self.advance(); Ok(Expr::Ident("bool".to_string())) }
            _ => {
                let tok = self.peek_token().clone();
                Err(ParseError {
                    msg: format!("Unexpected token in expression: {:?}", tok.kind),
                    line: tok.line,
                    col: tok.col,
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParseError at {}:{}: {}", self.line, self.col, self.msg)
    }
}
