use crate::compiler::CompileError;

#[derive(Debug, Clone, PartialEq)]
pub enum LineKind { Normal, Debug, Warn, Error }

#[derive(Debug, Clone)]
pub struct ConsoleLine {
    pub kind: LineKind,
    pub text: String,
}

impl ConsoleLine {
    pub fn normal(t: impl Into<String>) -> Self { Self { kind: LineKind::Normal, text: t.into() } }
    pub fn debug_line(t: impl Into<String>) -> Self { Self { kind: LineKind::Debug,  text: t.into() } }
    pub fn error(t: impl Into<String>) -> Self { Self { kind: LineKind::Error,  text: t.into() } }
}

pub struct ConsoleState {
    pub lines: Vec<ConsoleLine>,
}

impl ConsoleState {
    pub fn new() -> Self { ConsoleState { lines: Vec::new() } }
    pub fn push(&mut self, line: ConsoleLine) { self.lines.push(line); }
    pub fn clear(&mut self) { self.lines.clear(); }
    pub fn push_compile_errors(&mut self, errors: &[CompileError]) {
        for e in errors {
            self.push(ConsoleLine::error(format!(
                "[{}:{}] {}",
                e.file.as_str(),
                e.line,
                e.message
            )));
        }
    }
}
