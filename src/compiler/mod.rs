pub mod lexer;
pub mod parser;
pub mod analyzer;
pub mod codegen;
pub mod cache;

use std::path::Path;
use crate::compiler::lexer::Lexer;
use crate::compiler::parser::Parser;
use crate::compiler::analyzer::Analyzer;
use crate::compiler::codegen::CodeGen;
use crate::compiler::cache::CompileCache;

/// コンパイル結果
#[derive(Debug)]
pub enum CompileResult {
    /// コンパイル成功: 生成されたRustコード
    Success(String),
    /// キャッシュヒット: 既存バイナリパス
    Cached(std::path::PathBuf),
    /// コンパイルエラー
    Error(Vec<CompileError>),
}

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub line: usize,
    pub col: usize,
    pub file: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}: {}", self.file, self.line, self.col, self.message)
    }
}

/// Mistralソースを解析してASTを取得（IDEのシンタックスハイライト用）
pub fn parse_source(source: &str) -> Result<Vec<parser::Stmt>, Vec<CompileError>> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = Parser::new(tokens);
    parser.parse().map_err(|e| vec![CompileError {
        message: e.msg,
        line: e.line,
        col: e.col,
        file: "unknown".to_string(),
    }])
}

/// フルコンパイルパイプライン
pub fn compile(
    source: &str,
    source_path: &Path,
    cache: &mut CompileCache,
    is_release: bool,
) -> CompileResult {
    let source_hash = CompileCache::hash_source(source);

    // キャッシュチェック（Runモード時のみ）
    if !is_release && cache.is_valid(source_path, &source_hash) {
        if let Some(entry) = cache.get(source_path) {
            return CompileResult::Cached(entry.binary_path.clone());
        }
    }

    // Lexer
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    // Parser
    let mut parser = Parser::new(tokens);
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => return CompileResult::Error(vec![CompileError {
            message: e.msg,
            line: e.line,
            col: e.col,
            file: source_path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        }]),
    };

    // Analyzer
    let mut analyzer = Analyzer::new();
    analyzer.analyze(&ast);
    if !analyzer.errors.is_empty() {
        let errors = analyzer.errors.iter().map(|e| CompileError {
            message: e.msg.clone(),
            line: e.line,
            col: 0,
            file: source_path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        }).collect();
        return CompileResult::Error(errors);
    }

    // CodeGen
    let mut codegen = CodeGen::new();
    let rust_code = codegen.generate(&ast);

    CompileResult::Success(rust_code)
}
