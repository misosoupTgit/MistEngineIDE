/// シンタックスハイライト（egui LayoutJob ベース）
/// テキスト編集状態は egui::TextEdit に任せる

use egui::text::{LayoutJob, TextFormat};
use egui::FontId;
use crate::compiler::lexer::{Lexer, TokenKind};
use crate::ide::theme::Theme;
use std::path::PathBuf;

// ──────────────────────────────────────
// 着色ロジック
// ──────────────────────────────────────

fn is_builtin(name: &str) -> bool {
    matches!(name,
        "print"|"printf"|"debug"|"str"|"int"|"float"|"bool"|
        "len"|"typeof"|"range"|"math"|"draw"|"image"|"input"|
        "button"|"Color"|"Key"|"Controller"
    )
}

/// トークンの表示テキストを再現
fn tok_text(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Ident(s)    => s.clone(),
        TokenKind::IntLit(n)   => n.to_string(),
        TokenKind::FloatLit(f) => f.to_string(),
        TokenKind::StrLit(s)   => format!("\"{}\"", s),
        TokenKind::BoolLit(b)  => b.to_string(),
        TokenKind::Null        => "null".into(),
        TokenKind::Let         => "let".into(),
        TokenKind::Func        => "func".into(),
        TokenKind::Return      => "return".into(),
        TokenKind::If          => "if".into(),
        TokenKind::Ifelse      => "ifelse".into(),
        TokenKind::Else        => "else".into(),
        TokenKind::While       => "while".into(),
        TokenKind::For         => "for".into(),
        TokenKind::In          => "in".into(),
        TokenKind::Range       => "range".into(),
        TokenKind::Switch      => "switch".into(),
        TokenKind::Case        => "case".into(),
        TokenKind::Default     => "default".into(),
        TokenKind::Try         => "try".into(),
        TokenKind::Catch       => "catch".into(),
        TokenKind::Import      => "import".into(),
        TokenKind::Repeat      => "repeat".into(),
        TokenKind::Clone       => "clone".into(),
        TokenKind::As          => "as".into(),
        TokenKind::Break       => "break".into(),
        TokenKind::Continue    => "continue".into(),
        TokenKind::And         => "and".into(),
        TokenKind::Or          => "or".into(),
        TokenKind::Not         => "not".into(),
        TokenKind::TypeInt     => "int".into(),
        TokenKind::TypeFloat   => "float".into(),
        TokenKind::TypeStr     => "str".into(),
        TokenKind::TypeBool    => "bool".into(),
        TokenKind::TypeList    => "list".into(),
        TokenKind::TypeMap     => "map".into(),
        TokenKind::Plus        => "+".into(),
        TokenKind::Minus       => "-".into(),
        TokenKind::Star        => "*".into(),
        TokenKind::Slash       => "/".into(),
        TokenKind::Percent     => "%".into(),
        TokenKind::StarStar    => "**".into(),
        TokenKind::Eq          => "==".into(),
        TokenKind::NotEq       => "!=".into(),
        TokenKind::Lt          => "<".into(),
        TokenKind::Gt          => ">".into(),
        TokenKind::LtEq        => "<=".into(),
        TokenKind::GtEq        => ">=".into(),
        TokenKind::Assign      => "=".into(),
        TokenKind::PlusAssign  => "+=".into(),
        TokenKind::MinusAssign => "-=".into(),
        TokenKind::StarAssign  => "*=".into(),
        TokenKind::SlashAssign => "/=".into(),
        TokenKind::PlusPlus    => "++".into(),
        TokenKind::MinusMinus  => "--".into(),
        TokenKind::Arrow       => "->".into(),
        TokenKind::LParen      => "(".into(),
        TokenKind::RParen      => ")".into(),
        TokenKind::LBrace      => "{".into(),
        TokenKind::RBrace      => "}".into(),
        TokenKind::LBracket    => "[".into(),
        TokenKind::RBracket    => "]".into(),
        TokenKind::Comma       => ",".into(),
        TokenKind::Colon       => ":".into(),
        TokenKind::Semicolon   => ";".into(),
        TokenKind::Dot         => ".".into(),
        _ => String::new(),
    }
}

/// 1行をトークナイズし、(byte_start, byte_end, color) のリストを返す
fn colorize_line(line: &str, prev_func: &mut bool, theme: &Theme) -> Vec<(usize, usize, egui::Color32)> {
    if line.trim_start().starts_with("//") {
        return vec![(0, line.len(), theme.syn_comment)];
    }
    let mut lexer = Lexer::new(line);
    let tokens = lexer.tokenize();
    let mut result = Vec::new();
    let mut cursor = 0usize;

    for tok in &tokens {
        if matches!(tok.kind, TokenKind::Eof | TokenKind::Newline) { break; }
        let txt = tok_text(&tok.kind);
        if txt.is_empty() { continue; }
        if let Some(rel) = line[cursor..].find(txt.as_str()) {
            let start = cursor + rel;
            let end   = start + txt.len();
            if start > cursor {
                result.push((cursor, start, theme.text)); // gap (whitespace)
            }
            let color = match &tok.kind {
                TokenKind::Ident(name) => {
                    let pf = *prev_func;
                    *prev_func = false;
                    if pf { theme.syn_func }
                    else if is_builtin(name) { theme.syn_builtin }
                    else { theme.syn_variable }
                }
                k => {
                    *prev_func = matches!(k, TokenKind::Func);
                    keyword_color(k, theme)
                }
            };
            result.push((start, end, color));
            cursor = end;
        }
    }
    if cursor < line.len() {
        result.push((cursor, line.len(), theme.text));
    }
    result
}

fn keyword_color(kind: &TokenKind, t: &Theme) -> egui::Color32 {
    match kind {
        TokenKind::Let|TokenKind::Func|TokenKind::Return|
        TokenKind::If|TokenKind::Ifelse|TokenKind::Else|
        TokenKind::While|TokenKind::For|TokenKind::In|
        TokenKind::Range|TokenKind::Switch|TokenKind::Case|
        TokenKind::Default|TokenKind::Try|TokenKind::Catch|
        TokenKind::Import|TokenKind::Repeat|TokenKind::Clone|
        TokenKind::As|TokenKind::Break|TokenKind::Continue|
        TokenKind::And|TokenKind::Or|TokenKind::Not
        => t.syn_keyword,
        TokenKind::TypeInt|TokenKind::TypeFloat|TokenKind::TypeStr|
        TokenKind::TypeBool|TokenKind::TypeList|TokenKind::TypeMap
        => t.syn_type,
        TokenKind::BoolLit(_)|TokenKind::Null => t.syn_literal,
        TokenKind::StrLit(_) => t.syn_string,
        TokenKind::IntLit(_)|TokenKind::FloatLit(_) => t.syn_number,
        TokenKind::Plus|TokenKind::Minus|TokenKind::Star|
        TokenKind::Slash|TokenKind::Percent|TokenKind::StarStar|
        TokenKind::Eq|TokenKind::NotEq|TokenKind::Lt|TokenKind::Gt|
        TokenKind::LtEq|TokenKind::GtEq|TokenKind::Assign|
        TokenKind::PlusAssign|TokenKind::MinusAssign|TokenKind::StarAssign|
        TokenKind::SlashAssign|TokenKind::PlusPlus|TokenKind::MinusMinus|
        TokenKind::Arrow => t.syn_operator,
        _ => t.text,
    }
}

/// テキスト全体から egui::text::LayoutJob を生成（着色済み）
pub fn build_layout_job(text: &str, theme: &Theme) -> LayoutJob {
    let font = FontId::monospace(14.0);
    let mut job = LayoutJob::default();
    let mut prev_func = false;

    for (li, line) in text.split('\n').enumerate() {
        if li > 0 {
            job.append("\n", 0.0, TextFormat { font_id: font.clone(), color: theme.text, ..Default::default() });
        }
        if line.is_empty() { continue; }
        let spans = colorize_line(line, &mut prev_func, theme);
        if spans.is_empty() {
            job.append(line, 0.0, TextFormat { font_id: font.clone(), color: theme.text, ..Default::default() });
        } else {
            for (start, end, color) in spans {
                let slice = &line[start..end];
                if !slice.is_empty() {
                    job.append(slice, 0.0, TextFormat { font_id: font.clone(), color, ..Default::default() });
                }
            }
        }
    }
    job
}

// ──────────────────────────────────────
// エディタ状態（ファイルパス・dirty管理のみ）
// テキスト本体は egui::TextEdit が管理する
// ──────────────────────────────────────

pub struct EditorState {
    pub text: String,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
}

impl EditorState {
    pub fn new() -> Self {
        EditorState { text: String::new(), file_path: None, dirty: false }
    }
    pub fn load_file(&mut self, path: &PathBuf) -> std::io::Result<()> {
        self.text = std::fs::read_to_string(path)?;
        self.file_path = Some(path.clone());
        self.dirty = false;
        Ok(())
    }
    pub fn save_file(&mut self) -> std::io::Result<()> {
        if let Some(p) = &self.file_path.clone() {
            std::fs::write(p, &self.text)?;
            self.dirty = false;
        }
        Ok(())
    }
}
