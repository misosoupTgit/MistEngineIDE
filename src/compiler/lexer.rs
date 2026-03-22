/// Mistral言語 Lexer
/// GDScript構文をベースとした字句解析器

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // リテラル
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    BoolLit(bool),
    Null,

    // 識別子
    Ident(String),

    // キーワード
    Let,
    Func,
    Return,
    If,
    Ifelse,
    Else,
    While,
    For,
    In,
    Range,
    Switch,
    Case,
    Default,
    Try,
    Catch,
    Import,
    Repeat,
    Clone,
    As,
    Break,
    Continue,

    // 型名
    TypeInt,
    TypeFloat,
    TypeStr,
    TypeBool,
    TypeList,
    TypeMap,

    // 演算子
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,  // **
    Eq,        // ==
    NotEq,     // !=
    Lt,        // <
    Gt,        // >
    LtEq,      // <=
    GtEq,      // >=
    And,       // and / &&
    Or,        // or / ||
    Not,       // not / !
    Assign,    // =
    PlusAssign,   // +=
    MinusAssign,  // -=
    StarAssign,   // *=
    SlashAssign,  // /=
    PlusPlus,     // ++
    MinusMinus,   // --
    Arrow,        // ->

    // デリミタ
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Dot,

    // コメント（読み飛ばし）
    // NewLine
    Newline,

    // EOF
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.source.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        // 既に /* を消費済み
        loop {
            match self.peek() {
                None => break,
                Some('*') if self.peek2() == Some('/') => {
                    self.advance();
                    self.advance();
                    break;
                }
                _ => { self.advance(); }
            }
        }
    }

    fn read_string(&mut self) -> TokenKind {
        let mut s = String::new();
        loop {
            match self.advance() {
                None | Some('"') => break,
                Some('\\') => {
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some(c) => { s.push('\\'); s.push(c); }
                        None => break,
                    }
                }
                Some(c) => s.push(c),
            }
        }
        TokenKind::StrLit(s)
    }

    fn read_number(&mut self, first: char) -> TokenKind {
        let mut num = String::from(first);
        let mut is_float = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                num.push(c);
                self.advance();
            } else if c == '.' && self.peek2().map(|x| x.is_ascii_digit()).unwrap_or(false) {
                is_float = true;
                num.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if is_float {
            TokenKind::FloatLit(num.parse().unwrap_or(0.0))
        } else {
            TokenKind::IntLit(num.parse().unwrap_or(0))
        }
    }

    fn read_ident(&mut self, first: char) -> TokenKind {
        let mut ident = String::from(first);
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match ident.as_str() {
            "let"      => TokenKind::Let,
            "func"     => TokenKind::Func,
            "return"   => TokenKind::Return,
            "if"       => TokenKind::If,
            "ifelse"   => TokenKind::Ifelse,
            "else"     => TokenKind::Else,
            "while"    => TokenKind::While,
            "for"      => TokenKind::For,
            "in"       => TokenKind::In,
            "range"    => TokenKind::Range,
            "switch"   => TokenKind::Switch,
            "case"     => TokenKind::Case,
            "default"  => TokenKind::Default,
            "try"      => TokenKind::Try,
            "catch"    => TokenKind::Catch,
            "import"   => TokenKind::Import,
            "repeat"   => TokenKind::Repeat,
            "clone"    => TokenKind::Clone,
            "as"       => TokenKind::As,
            "break"    => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "true"     => TokenKind::BoolLit(true),
            "false"    => TokenKind::BoolLit(false),
            "null"     => TokenKind::Null,
            "and"      => TokenKind::And,
            "or"       => TokenKind::Or,
            "not"      => TokenKind::Not,
            "int"      => TokenKind::TypeInt,
            "float"    => TokenKind::TypeFloat,
            "str"      => TokenKind::TypeStr,
            "bool"     => TokenKind::TypeBool,
            "list"     => TokenKind::TypeList,
            "map"      => TokenKind::TypeMap,
            _          => TokenKind::Ident(ident),
        }
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            let line = self.line;
            let col = self.col;

            let ch = match self.peek() {
                None => {
                    tokens.push(Token { kind: TokenKind::Eof, line, col });
                    break;
                }
                Some(c) => c,
            };

            // 改行
            if ch == '\n' {
                self.advance();
                tokens.push(Token { kind: TokenKind::Newline, line, col });
                continue;
            }

            self.advance();

            let kind = match ch {
                // コメント
                '/' if self.peek() == Some('/') => {
                    self.advance();
                    self.skip_line_comment();
                    continue;
                }
                '/' if self.peek() == Some('*') => {
                    self.advance();
                    self.skip_block_comment();
                    continue;
                }
                '"' => self.read_string(),
                c if c.is_ascii_digit() => self.read_number(c),
                c if c.is_alphabetic() || c == '_' => self.read_ident(c),

                '+' => {
                    if self.peek() == Some('+') { self.advance(); TokenKind::PlusPlus }
                    else if self.peek() == Some('=') { self.advance(); TokenKind::PlusAssign }
                    else { TokenKind::Plus }
                }
                '-' => {
                    if self.peek() == Some('-') { self.advance(); TokenKind::MinusMinus }
                    else if self.peek() == Some('=') { self.advance(); TokenKind::MinusAssign }
                    else if self.peek() == Some('>') { self.advance(); TokenKind::Arrow }
                    else { TokenKind::Minus }
                }
                '*' => {
                    if self.peek() == Some('*') { self.advance(); TokenKind::StarStar }
                    else if self.peek() == Some('=') { self.advance(); TokenKind::StarAssign }
                    else { TokenKind::Star }
                }
                '/' => {
                    if self.peek() == Some('=') { self.advance(); TokenKind::SlashAssign }
                    else { TokenKind::Slash }
                }
                '%' => TokenKind::Percent,
                '=' => {
                    if self.peek() == Some('=') { self.advance(); TokenKind::Eq }
                    else { TokenKind::Assign }
                }
                '!' => {
                    if self.peek() == Some('=') { self.advance(); TokenKind::NotEq }
                    else { TokenKind::Not }
                }
                '<' => {
                    if self.peek() == Some('=') { self.advance(); TokenKind::LtEq }
                    else { TokenKind::Lt }
                }
                '>' => {
                    if self.peek() == Some('=') { self.advance(); TokenKind::GtEq }
                    else { TokenKind::Gt }
                }
                '&' if self.peek() == Some('&') => { self.advance(); TokenKind::And }
                '|' if self.peek() == Some('|') => { self.advance(); TokenKind::Or }
                '(' => TokenKind::LParen,
                ')' => TokenKind::RParen,
                '{' => TokenKind::LBrace,
                '}' => TokenKind::RBrace,
                '[' => TokenKind::LBracket,
                ']' => TokenKind::RBracket,
                ',' => TokenKind::Comma,
                ':' => TokenKind::Colon,
                ';' => TokenKind::Semicolon,
                '.' => TokenKind::Dot,
                _ => continue, // 不明な文字は読み飛ばし
            };
            tokens.push(Token { kind, line, col });
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let src = "let x = 10";
        let mut lex = Lexer::new(src);
        let toks = lex.tokenize();
        assert!(matches!(toks[0].kind, TokenKind::Let));
        assert!(matches!(toks[1].kind, TokenKind::Ident(_)));
        assert!(matches!(toks[2].kind, TokenKind::Assign));
        assert!(matches!(toks[3].kind, TokenKind::IntLit(10)));
    }

    #[test]
    fn test_func() {
        let src = "func update(delta) {}";
        let mut lex = Lexer::new(src);
        let toks = lex.tokenize();
        assert!(matches!(toks[0].kind, TokenKind::Func));
        assert!(matches!(toks[1].kind, TokenKind::Ident(_)));
    }
}
