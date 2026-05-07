use crate::compiler::parser::{AssignOp, BinOpKind, Expr, Stmt, UnaryOpKind};
/// Mistral ツリー走行インタープリター
/// cargo build 不要・即時起動（GDScript 相当の速度）
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

// ── 値型 ─────────────────────────────────────────────────────
#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Func {
        params: Vec<String>,
        body: Vec<Stmt>,
        closure: HashMap<String, Value>,
    },
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
    pub fn as_float(&self) -> f64 {
        match self {
            Value::Int(n) => *n as f64,
            Value::Float(f) => *f,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }
    pub fn as_int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Float(f) => *f as i64,
            Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        }
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::Bool(_) => "bool",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Func { .. } => "func",
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(v) => write!(f, "{}", v),
            Value::Str(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::List(v) => {
                write!(f, "[")?;
                for (i, x) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", x)?;
                }
                write!(f, "]")
            }
            Value::Map(_) => write!(f, "{{...}}"),
            Value::Func { .. } => write!(f, "<func>"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
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

// ── 描画コマンド ──────────────────────────────────────────────
#[derive(Clone, Debug)]
pub enum DrawCmd {
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: [f32; 4],
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    Square {
        x: f32,
        y: f32,
        s: f32,
        color: [f32; 4],
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: [f32; 4],
    },
    Triangle {
        x: f32,
        y: f32,
        s: f32,
        color: [f32; 4],
    },
    Polygon {
        x: f32,
        y: f32,
        s: f32,
        sides: u32,
        color: [f32; 4],
    },
    Diamond {
        x: f32,
        y: f32,
        s: f32,
        color: [f32; 4],
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: [f32; 4],
    },
    Image {
        x: f32,
        y: f32,
        path: String,
        w: f32,
        h: f32,
    },
    Background([f32; 4]),
}

// ── スレッド間共有ゲーム状態 ──────────────────────────────────
pub struct GameState {
    pub draw_cmds: Arc<Mutex<Vec<DrawCmd>>>,
    pub bg_color: Arc<Mutex<[f32; 4]>>,
    pub held_keys: Arc<Mutex<std::collections::HashSet<String>>>,
    pub running: Arc<AtomicBool>,
    pub console: Arc<Mutex<Vec<String>>>,
    pub fps: Arc<AtomicU32>,
    pub screen_w: Arc<AtomicU32>,
    pub screen_h: Arc<AtomicU32>,
    /// スクリプトスレッドが draw() を完了するたびにインクリメント。
    /// レンダースレッドは最後描画時の値と比較し同一なら描画をスキップ。
    pub frame_id: Arc<std::sync::atomic::AtomicU64>,
}

impl GameState {
    pub fn new() -> Self {
        GameState {
            draw_cmds: Arc::new(Mutex::new(Vec::new())),
            bg_color: Arc::new(Mutex::new([0.05, 0.05, 0.1, 1.0])),
            held_keys: Arc::new(Mutex::new(Default::default())),
            running: Arc::new(AtomicBool::new(true)),
            console: Arc::new(Mutex::new(Vec::new())),
            fps: Arc::new(AtomicU32::new(0)),
            screen_w: Arc::new(AtomicU32::new(0)),
            screen_h: Arc::new(AtomicU32::new(0)),
            frame_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }
    pub fn clone_arcs(&self) -> Self {
        GameState {
            draw_cmds: Arc::clone(&self.draw_cmds),
            bg_color: Arc::clone(&self.bg_color),
            held_keys: Arc::clone(&self.held_keys),
            running: Arc::clone(&self.running),
            console: Arc::clone(&self.console),
            fps: Arc::clone(&self.fps),
            screen_w: Arc::clone(&self.screen_w),
            screen_h: Arc::clone(&self.screen_h),
            frame_id: Arc::clone(&self.frame_id),
        }
    }
}

// ── 制御フロー ────────────────────────────────────────────────
#[derive(Debug)]
pub enum Signal {
    Return(Value),
    Break,
    Continue,
}

// ── インタープリター ───────────────────────────────────────
pub struct Interpreter {
    env: Vec<HashMap<String, Value>>,
    state: GameState,
    /// draw() 中の描画コマンドをローカル蓄積するバッファ
    /// draw() 完了後に state.draw_cmds へ一括スワップする
    pub frame_cmds: Vec<DrawCmd>,
}

impl Interpreter {
    pub fn new(state: GameState) -> Self {
        Interpreter {
            env: vec![HashMap::new()],
            state,
            frame_cmds: Vec::new(),
        }
    }

    // ─ スコープ ─
    pub fn get_var(&self, name: &str) -> Option<Value> {
        for scope in self.env.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }
    fn set_var(&mut self, name: &str, val: Value) {
        for scope in self.env.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return;
            }
        }
        if let Some(s) = self.env.last_mut() {
            s.insert(name.to_string(), val);
        }
    }
    fn declare_var(&mut self, name: &str, val: Value) {
        if let Some(s) = self.env.last_mut() {
            s.insert(name.to_string(), val);
        }
    }
    fn push_scope(&mut self) {
        self.env.push(HashMap::new());
    }
    fn pop_scope(&mut self) {
        self.env.pop();
    }

    // ─ 色変換 ─
    // 受け付けるフォーマット:
    //   文字列: "#RRGGBB" / "#RRGGBBAA" / "#RGB" / "#RGBA"  (# は省略可)
    //   リスト: [r, g, b] / [r, g, b, a]
    //     ・各要素が 1.0 より大きい → 0〜255 スケールとして自動正規化
    //     ・1.0 以下              → 0.0〜1.0 スケールとしてそのまま使用
    fn extract_color(&self, v: &Value) -> [f32; 4] {
        match v {
            // ── 文字列 HEX ──
            Value::Str(s) => parse_hex_color(s).unwrap_or([1.0, 1.0, 1.0, 1.0]),

            // ── リスト（3 or 4 要素）──
            Value::List(lst) if lst.len() >= 3 => {
                // いずれかの要素が 1.0 より大きければ 0-255 スケールと判定
                let is_byte = lst.iter().any(|v| v.as_float() > 1.0);
                let norm = |v: &Value| -> f32 {
                    let f = v.as_float() as f32;
                    if is_byte { (f / 255.0).clamp(0.0, 1.0) } else { f.clamp(0.0, 1.0) }
                };
                let a = if lst.len() >= 4 { norm(&lst[3]) }
                        else if is_byte { 1.0 } else { lst.get(3).map(norm).unwrap_or(1.0) };
                [norm(&lst[0]), norm(&lst[1]), norm(&lst[2]), a]
            }

            _ => [1.0, 1.0, 1.0, 1.0],
        }
    }

    // ─ 式評価 ─
    pub fn eval(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::IntLit(n) => Ok(Value::Int(*n)),
            Expr::FloatLit(f) => Ok(Value::Float(*f)),
            Expr::StrLit(s) => Ok(Value::Str(s.clone())),
            Expr::BoolLit(b) => Ok(Value::Bool(*b)),
            Expr::Null => Ok(Value::Null),
            Expr::ListLit(items) => {
                let v: Result<Vec<_>, _> = items.iter().map(|e| self.eval(e)).collect();
                Ok(Value::List(v?))
            }
            Expr::Ident(name) => {
                // Color / math 定数
                if let Some(v) = self.member_const(name, "") {
                    return Ok(v);
                }
                self.get_var(name)
                    .ok_or_else(|| format!("未定義の変数: {}", name))
            }
            Expr::BinOp { op, lhs, rhs } => self.eval_binop(op, lhs, rhs),
            Expr::UnaryOp { op, expr } => {
                let v = self.eval(expr)?;
                match op {
                    UnaryOpKind::Neg => match v {
                        Value::Int(n) => Ok(Value::Int(-n)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        _ => Err("数値以外への単項マイナス".into()),
                    },
                    UnaryOpKind::Not => Ok(Value::Bool(!v.is_truthy())),
                }
            }
            Expr::Call {
                callee,
                args,
                kwargs,
            } => self.eval_call(callee, args, kwargs),
            Expr::Member { obj, field } => {
                if let Expr::Ident(n) = obj.as_ref() {
                    if let Some(v) = self.member_const(n, field) {
                        return Ok(v);
                    }
                }
                let obj_val = self.eval(obj)?;
                match obj_val {
                    Value::Map(m) => m
                        .get(field.as_str())
                        .cloned()
                        .ok_or_else(|| format!("フィールド未定義: {}", field)),
                    _ => Err(format!("メンバアクセスは Map のみ: .{}", field)),
                }
            }
            Expr::Index { obj, idx } => {
                let ov = self.eval(obj)?;
                let i = self.eval(idx)?.as_int();
                match ov {
                    Value::List(v) => {
                        let ui = if i < 0 {
                            v.len().saturating_sub((-i) as usize)
                        } else {
                            i as usize
                        };
                        v.get(ui).cloned().ok_or_else(|| format!("範囲外: {}", i))
                    }
                    Value::Str(s) => s
                        .chars()
                        .nth(i as usize)
                        .map(|c| Value::Str(c.to_string()))
                        .ok_or_else(|| format!("範囲外: {}", i)),
                    _ => Err("インデックスは List/Str のみ".into()),
                }
            }
            Expr::Lambda { params, body, .. } => {
                let names = params.iter().map(|(n, _)| n.clone()).collect();
                let closure: HashMap<_, _> = self
                    .env
                    .iter()
                    .flat_map(|s| s.iter().map(|(k, v)| (k.clone(), v.clone())))
                    .collect();
                Ok(Value::Func {
                    params: names,
                    body: body.clone(),
                    closure,
                })
            }
        }
    }

    fn member_const(&self, obj: &str, field: &str) -> Option<Value> {
        let c = |r: f64, g: f64, b: f64, a: f64| {
            Value::List(vec![
                Value::Float(r),
                Value::Float(g),
                Value::Float(b),
                Value::Float(a),
            ])
        };
        match (obj, field) {
            ("Color", "RED") => Some(c(1., 0., 0., 1.)),
            ("Color", "GREEN") => Some(c(0., 1., 0., 1.)),
            ("Color", "BLUE") => Some(c(0., 0., 1., 1.)),
            ("Color", "WHITE") => Some(c(1., 1., 1., 1.)),
            ("Color", "BLACK") => Some(c(0., 0., 0., 1.)),
            ("Color", "YELLOW") => Some(c(1., 1., 0., 1.)),
            ("Color", "CYAN") => Some(c(0., 1., 1., 1.)),
            ("Color", "MAGENTA") => Some(c(1., 0., 1., 1.)),
            ("math", "PI") => Some(Value::Float(std::f64::consts::PI)),
            ("math", "TAU") => Some(Value::Float(std::f64::consts::TAU)),
            ("math", "E") => Some(Value::Float(std::f64::consts::E)),
            ("math", "INF") => Some(Value::Float(f64::INFINITY)),
            _ => None,
        }
    }

    fn eval_binop(&mut self, op: &BinOpKind, lhs: &Expr, rhs: &Expr) -> Result<Value, String> {
        // short-circuit
        if matches!(op, BinOpKind::And) {
            let l = self.eval(lhs)?;
            if !l.is_truthy() {
                return Ok(Value::Bool(false));
            }
            return Ok(Value::Bool(self.eval(rhs)?.is_truthy()));
        }
        if matches!(op, BinOpKind::Or) {
            let l = self.eval(lhs)?;
            if l.is_truthy() {
                return Ok(Value::Bool(true));
            }
            return Ok(Value::Bool(self.eval(rhs)?.is_truthy()));
        }
        let l = self.eval(lhs)?;
        let r = self.eval(rhs)?;
        match op {
            BinOpKind::Add => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                (Value::Str(a), _) => Ok(Value::Str(format!("{}{}", a, r))),
                _ => Ok(Value::Float(l.as_float() + r.as_float())),
            },
            BinOpKind::Sub => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
                _ => Ok(Value::Float(l.as_float() - r.as_float())),
            },
            BinOpKind::Mul => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
                _ => Ok(Value::Float(l.as_float() * r.as_float())),
            },
            BinOpKind::Div => {
                let d = r.as_float();
                if d == 0.0 {
                    return Err("ゼロ除算".into());
                }
                match (&l, &r) {
                    (Value::Int(a), Value::Int(b)) if *b != 0 => Ok(Value::Int(a / b)),
                    _ => Ok(Value::Float(l.as_float() / d)),
                }
            }
            BinOpKind::Mod => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) if *b != 0 => Ok(Value::Int(a % b)),
                _ => Ok(Value::Float(l.as_float() % r.as_float())),
            },
            BinOpKind::Pow => Ok(Value::Float(l.as_float().powf(r.as_float()))),
            BinOpKind::Eq => Ok(Value::Bool(l == r)),
            BinOpKind::NotEq => Ok(Value::Bool(l != r)),
            BinOpKind::Lt => Ok(Value::Bool(l.as_float() < r.as_float())),
            BinOpKind::Gt => Ok(Value::Bool(l.as_float() > r.as_float())),
            BinOpKind::LtEq => Ok(Value::Bool(l.as_float() <= r.as_float())),
            BinOpKind::GtEq => Ok(Value::Bool(l.as_float() >= r.as_float())),
            BinOpKind::And | BinOpKind::Or => unreachable!(),
        }
    }

    fn eval_call(
        &mut self,
        callee: &Expr,
        args: &[Expr],
        kwargs: &[(String, Expr)],
    ) -> Result<Value, String> {
        // draw.* / math.* / input.*
        if let Expr::Member { obj, field } = callee {
            if let Expr::Ident(ns) = obj.as_ref() {
                match ns.as_str() {
                    "draw" => return self.call_draw(field, args, kwargs),
                    "math" => return self.call_math(field, args),
                    "input" => return self.call_input(field, args),
                    "engine" => return self.call_engine(field, args),
                    _ => {}
                }
            }
        }
        // 通常の関数
        if let Expr::Ident(name) = callee {
            let av: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
            let av = av?;
            // 組み込み
            if let Some(r) = self.call_builtin(name, &av, kwargs) {
                return r;
            }
            // ユーザー定義
            if let Some(func) = self.get_var(name) {
                return self.call_value(func, av);
            }
            return Err(format!("未定義の関数: {}", name));
        }
        let func = self.eval(callee)?;
        let av: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
        self.call_value(func, av?)
    }

    pub fn call_value(&mut self, func: Value, args: Vec<Value>) -> Result<Value, String> {
        match func {
            Value::Func {
                params,
                body,
                closure,
            } => {
                self.push_scope();
                for (k, v) in closure {
                    self.declare_var(&k, v);
                }
                for (i, p) in params.iter().enumerate() {
                    self.declare_var(p, args.get(i).cloned().unwrap_or(Value::Null));
                }
                let result = self.exec_stmts(&body);
                self.pop_scope();
                match result {
                    Ok(Some(Signal::Return(v))) => Ok(v),
                    Ok(_) => Ok(Value::Null),
                    Err(e) => Err(e),
                }
            }
            _ => Err("関数以外を呼び出しました".into()),
        }
    }

    fn call_builtin(
        &mut self,
        name: &str,
        av: &[Value],
        kwargs: &[(String, Expr)],
    ) -> Option<Result<Value, String>> {
        Some(match name {
            "print" => {
                let s = av
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                println!("{}", s);
                if let Ok(mut q) = self.state.console.lock() {
                    q.push(s);
                }
                Ok(Value::Null)
            }
            "debug" => {
                eprintln!(
                    "[debug] {}",
                    av.iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                Ok(Value::Null)
            }
            "len" => Ok(Value::Int(match av.first() {
                Some(Value::List(v)) => v.len() as i64,
                Some(Value::Str(s)) => s.len() as i64,
                _ => 0,
            })),
            "str" => Ok(Value::Str(
                av.first().map(|v| v.to_string()).unwrap_or_default(),
            )),
            "int" => Ok(Value::Int(av.first().map(|v| v.as_int()).unwrap_or(0))),
            "float" => Ok(Value::Float(
                av.first().map(|v| v.as_float()).unwrap_or(0.0),
            )),
            "bool" => Ok(Value::Bool(
                av.first().map(|v| v.is_truthy()).unwrap_or(false),
            )),
            "typeof" => Ok(Value::Str(
                av.first()
                    .map(|v| v.type_name())
                    .unwrap_or("null")
                    .to_string(),
            )),
            "range" => {
                let (s, e) = if av.len() >= 2 {
                    (av[0].as_int(), av[1].as_int())
                } else {
                    (0, av.first().map(|v| v.as_int()).unwrap_or(0))
                };
                Ok(Value::List((s..e).map(Value::Int).collect()))
            }
            "wait" => {
                // 指定した秒数だけ処理を一時停止（game.sleep相当）
                let secs = av.first().map(|v| v.as_float()).unwrap_or(0.0);
                if secs > 0.0 {
                    std::thread::sleep(std::time::Duration::from_secs_f64(secs));
                }
                Ok(Value::Null)
            }

            // ── rotate(current_angle, delta_angle?) ─────────────────────
            // 現在の角度に delta_angle を加算し、0〜360 の範囲に正規化して返す
            // delta_angle 省略時は 0（回転なし）
            // 例: player_angle = rotate(player_angle, 15.0)
            "rotate" => {
                let current = av.get(0).map(|v| v.as_float()).unwrap_or(0.0);
                let delta = av.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                let new_angle = (current + delta).rem_euclid(360.0);
                Ok(Value::Float(new_angle))
            }

            // ── move_forward(x, y, steps, angle?) ───────────────────────
            // 指定した角度の方向へ steps だけ移動した [新x, 新y] を返す
            // angle 省略時は 90.0 度（画面右方向）
            //
            // 角度の定義（Scratch 互換）:
            //   0   = 上 (y-)
            //   90  = 右 (x+)  ← デフォルト
            //   180 = 下 (y+)
            //   270 = 左 (x-)
            //
            // 例: let pos = move_forward(player_x, player_y, 5.0, player_angle)
            //     player_x = pos[0]
            //     player_y = pos[1]
            "move_forward" => {
                let x = av.get(0).map(|v| v.as_float()).unwrap_or(0.0);
                let y = av.get(1).map(|v| v.as_float()).unwrap_or(0.0);
                let steps = av.get(2).map(|v| v.as_float()).unwrap_or(0.0);
                // angle 引数が省略されていれば 90.0 度（右）をデフォルトとする
                let angle_deg = av.get(3).map(|v| v.as_float()).unwrap_or(90.0);
                // Scratch 互換: 0度=上, 90度=右 → ラジアン変換
                // x 成分 = sin(θ), y 成分 = -cos(θ)  ← Y 軸が下向きの画面座標系
                let rad = angle_deg.to_radians();
                let new_x = x + steps * rad.sin();
                let new_y = y + steps * (-rad.cos());
                Ok(Value::List(vec![Value::Float(new_x), Value::Float(new_y)]))
            }

            _ => return None,
        })
    }

    fn call_draw(
        &mut self,
        func: &str,
        args: &[Expr],
        kwargs: &[(String, Expr)],
    ) -> Result<Value, String> {
        let av: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
        let av = av?;

        // kwarg "color" を優先。なければ最後の位置引数を色として解釈
        let color_val: Option<Value> =
            if let Some((_, ce)) = kwargs.iter().find(|(k, _)| k == "color") {
                Some(self.eval(ce)?)
            } else {
                None
            };
        let color = if let Some(ref cv) = color_val {
            self.extract_color(cv)
        } else {
            av.last()
                .map(|c| self.extract_color(c))
                .unwrap_or([1., 1., 1., 1.])
        };

        let f = |i: usize| av.get(i).map(|v| v.as_float() as f32).unwrap_or(0.0);
        let cmd = match func {
            "circle" => Some(DrawCmd::Circle {
                x: f(0),
                y: f(1),
                r: f(2),
                color,
            }),
            "rect" => Some(DrawCmd::Rect {
                x: f(0),
                y: f(1),
                w: f(2),
                h: f(3),
                color,
            }),
            "square" => Some(DrawCmd::Square {
                x: f(0),
                y: f(1),
                s: f(2),
                color,
            }),
            "line" => Some(DrawCmd::Line {
                x1: f(0),
                y1: f(1),
                x2: f(2),
                y2: f(3),
                color,
            }),
            "triangle" => Some(DrawCmd::Triangle {
                x: f(0),
                y: f(1),
                s: f(2),
                color,
            }),
            "polygon" => Some(DrawCmd::Polygon {
                x: f(0),
                y: f(1),
                s: f(2),
                sides: av.get(3).map(|v| v.as_int() as u32).unwrap_or(6),
                color,
            }),
            "diamond" => Some(DrawCmd::Diamond {
                x: f(0),
                y: f(1),
                s: f(2),
                color,
            }),
            "text" => {
                let text_str = av.get(2).map(|v| v.to_string()).unwrap_or_default();
                let size = av.get(3).map(|v| v.as_float() as f32).unwrap_or(24.0);
                Some(DrawCmd::Text {
                    x: f(0),
                    y: f(1),
                    text: text_str,
                    size,
                    color,
                })
            }
            "image" => {
                let path = av.get(2).map(|v| v.to_string()).unwrap_or_default();
                let w = av.get(3).map(|v| v.as_float() as f32).unwrap_or(0.0);
                let h = av.get(4).map(|v| v.as_float() as f32).unwrap_or(0.0);
                Some(DrawCmd::Image {
                    x: f(0),
                    y: f(1),
                    path,
                    w,
                    h,
                })
            }
            "background" => {
                *self.state.bg_color.lock().unwrap() = color;
                Some(DrawCmd::Background(color))
            }
            _ => None,
        };
        // ロックを取らずにローカルバッファへ蓄積
        if let Some(c) = cmd {
            self.frame_cmds.push(c);
        }
        Ok(Value::Null)
    }

    fn call_math(&mut self, func: &str, args: &[Expr]) -> Result<Value, String> {
        let av: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
        let av = av?;
        let f = |i: usize| av.get(i).map(|v| v.as_float()).unwrap_or(0.0);
        let r: f64 = match func {
            "sin" => f(0).sin(),
            "cos" => f(0).cos(),
            "tan" => f(0).tan(),
            "sqrt" => f(0).sqrt(),
            "abs" => f(0).abs(),
            "floor" => f(0).floor(),
            "ceil" => f(0).ceil(),
            "round" => f(0).round(),
            "log" => f(0).ln(),
            "sign" => f(0).signum(),
            "pow" => f(0).powf(f(1)),
            "max" => f(0).max(f(1)),
            "min" => f(0).min(f(1)),
            "clamp" => f(0).max(f(1)).min(f(2)),
            "lerp" => f(0) + (f(1) - f(0)) * f(2),
            "rand" => fast_rand(),
            "rand_int" => {
                let lo = f(0) as i64;
                let hi = f(1) as i64;
                return Ok(Value::Int(lo + (fast_rand() * (hi - lo) as f64) as i64));
            }
            _ => return Err(format!("未定義の math 関数: {}", func)),
        };
        Ok(Value::Float(r))
    }

    fn call_input(&mut self, func: &str, args: &[Expr]) -> Result<Value, String> {
        let av: Result<Vec<_>, _> = args.iter().map(|a| self.eval(a)).collect();
        let action = av?
            .into_iter()
            .next()
            .map(|v| v.to_string())
            .unwrap_or_default();
        let held = self.state.held_keys.lock().unwrap();
        // action_held / action_pressed / action_released はアクション名で照合
        // held / is_held / pressed なども同エイリアスとして扱う
        let r = match func {
            "held" | "is_held" | "pressed" | "is_action_pressed" | "action_held"
            | "action_pressed" => held.contains(&action),
            "released" | "action_released" => false, // released は held_keys には残らない
            _ => false,
        };
        Ok(Value::Bool(r))
    }

    fn call_engine(&mut self, func: &str, _args: &[Expr]) -> Result<Value, String> {
        match func {
            "fps" => Ok(Value::Int(self.state.fps.load(Ordering::Relaxed) as i64)),
            "width" => Ok(Value::Int(
                self.state.screen_w.load(Ordering::Relaxed) as i64
            )),
            "height" => Ok(Value::Int(
                self.state.screen_h.load(Ordering::Relaxed) as i64
            )),
            _ => Err(format!("未定義の engine 関数: {}", func)),
        }
    }

    // ─ 文実行 ─
    pub fn exec_stmts(&mut self, stmts: &[Stmt]) -> Result<Option<Signal>, String> {
        for stmt in stmts {
            if !self.state.running.load(Ordering::Relaxed) {
                break;
            }
            if let Some(sig) = self.exec_stmt(stmt)? {
                return Ok(Some(sig));
            }
        }
        Ok(None)
    }

    pub fn exec_stmt(&mut self, stmt: &Stmt) -> Result<Option<Signal>, String> {
        match stmt {
            Stmt::Import(_) => Ok(None),

            Stmt::VarDecl { name, init, .. } => {
                let v = init
                    .as_ref()
                    .map(|e| self.eval(e))
                    .transpose()?
                    .unwrap_or(Value::Null);
                self.declare_var(name, v);
                Ok(None)
            }

            Stmt::Assign { target, op, value } => {
                let rval = self.eval(value)?;
                self.do_assign(target, op, rval)?;
                Ok(None)
            }

            Stmt::Increment { target, delta } => {
                if let Expr::Ident(n) = target {
                    let cur = self.get_var(n).unwrap_or(Value::Int(0));
                    let new = match cur {
                        Value::Int(v) => Value::Int(v + delta),
                        Value::Float(v) => Value::Float(v + *delta as f64),
                        _ => Value::Int(*delta),
                    };
                    self.set_var(n, new);
                }
                Ok(None)
            }

            Stmt::FuncDef {
                name, params, body, ..
            } => {
                let names = params.iter().map(|(n, _)| n.clone()).collect();
                self.declare_var(
                    name,
                    Value::Func {
                        params: names,
                        body: body.clone(),
                        closure: HashMap::new(),
                    },
                );
                Ok(None)
            }

            Stmt::Return(expr) => {
                let v = expr
                    .as_ref()
                    .map(|e| self.eval(e))
                    .transpose()?
                    .unwrap_or(Value::Null);
                Ok(Some(Signal::Return(v)))
            }

            Stmt::If {
                cond,
                then_body,
                elseif_branches,
                else_body,
            } => {
                if self.eval(cond)?.is_truthy() {
                    self.push_scope();
                    let r = self.exec_stmts(then_body);
                    self.pop_scope();
                    return r;
                }
                for (ec, eb) in elseif_branches {
                    if self.eval(ec)?.is_truthy() {
                        self.push_scope();
                        let r = self.exec_stmts(eb);
                        self.pop_scope();
                        return r;
                    }
                }
                if let Some(eb) = else_body {
                    self.push_scope();
                    let r = self.exec_stmts(eb);
                    self.pop_scope();
                    return r;
                }
                Ok(None)
            }

            Stmt::While { cond, body } => {
                while self.eval(cond)?.is_truthy() && self.state.running.load(Ordering::Relaxed) {
                    self.push_scope();
                    let r = self.exec_stmts(body);
                    self.pop_scope();
                    match r? {
                        Some(Signal::Break) => break,
                        Some(Signal::Continue) => continue,
                        Some(s @ Signal::Return(_)) => return Ok(Some(s)),
                        None => {}
                    }
                }
                Ok(None)
            }

            Stmt::ForIn { var, iter, body } => {
                let items = match self.eval(iter)? {
                    Value::List(v) => v,
                    v => return Err(format!("for-in には List が必要: {:?}", v)),
                };
                for item in items {
                    self.push_scope();
                    self.declare_var(var, item);
                    match self.exec_stmts(body)? {
                        Some(Signal::Break) => {
                            self.pop_scope();
                            break;
                        }
                        Some(Signal::Continue) => {
                            self.pop_scope();
                            continue;
                        }
                        Some(s @ Signal::Return(_)) => {
                            self.pop_scope();
                            return Ok(Some(s));
                        }
                        None => {}
                    }
                    self.pop_scope();
                }
                Ok(None)
            }

            Stmt::ForRange {
                var,
                start,
                end,
                body,
            } => {
                let s = self.eval(start)?.as_int();
                let e = self.eval(end)?.as_int();
                for i in s..e {
                    self.push_scope();
                    self.declare_var(var, Value::Int(i));
                    match self.exec_stmts(body)? {
                        Some(Signal::Break) => {
                            self.pop_scope();
                            break;
                        }
                        Some(Signal::Continue) => {
                            self.pop_scope();
                            continue;
                        }
                        Some(s @ Signal::Return(_)) => {
                            self.pop_scope();
                            return Ok(Some(s));
                        }
                        None => {}
                    }
                    self.pop_scope();
                    if !self.state.running.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Ok(None)
            }

            Stmt::Repeat { count, var, body } => {
                let n = self.eval(count)?.as_int();
                for i in 0..n {
                    self.push_scope();
                    if let Some(v) = var {
                        self.declare_var(v, Value::Int(i));
                    }
                    match self.exec_stmts(body)? {
                        Some(Signal::Break) => {
                            self.pop_scope();
                            break;
                        }
                        Some(Signal::Continue) => {
                            self.pop_scope();
                            continue;
                        }
                        Some(s @ Signal::Return(_)) => {
                            self.pop_scope();
                            return Ok(Some(s));
                        }
                        None => {}
                    }
                    self.pop_scope();
                }
                Ok(None)
            }

            Stmt::TryCatch {
                try_body,
                catch_var,
                catch_body,
            } => {
                if let Err(e) = self.exec_stmts(try_body) {
                    self.push_scope();
                    self.declare_var(catch_var, Value::Str(e));
                    let r = self.exec_stmts(catch_body);
                    self.pop_scope();
                    return r;
                }
                Ok(None)
            }

            Stmt::Clone {
                expr,
                count,
                obj_var,
                idx_var,
                body,
            } => {
                let base = self.eval(expr)?;
                let n = self.eval(count)?.as_int();
                for i in 0..n {
                    self.push_scope();
                    self.declare_var(obj_var, base.clone());
                    self.declare_var(idx_var, Value::Int(i));
                    match self.exec_stmts(body)? {
                        Some(Signal::Break) => {
                            self.pop_scope();
                            break;
                        }
                        Some(Signal::Continue) => {
                            self.pop_scope();
                            continue;
                        }
                        Some(s @ Signal::Return(_)) => {
                            self.pop_scope();
                            return Ok(Some(s));
                        }
                        None => {}
                    }
                    self.pop_scope();
                }
                Ok(None)
            }

            Stmt::Switch {
                expr,
                cases,
                default,
            } => {
                let val = self.eval(expr)?;
                for (cv, cb) in cases {
                    if self.eval(cv)? == val {
                        self.push_scope();
                        let r = self.exec_stmts(cb);
                        self.pop_scope();
                        return r;
                    }
                }
                if let Some(db) = default {
                    self.push_scope();
                    let r = self.exec_stmts(db);
                    self.pop_scope();
                    return r;
                }
                Ok(None)
            }

            Stmt::Expr(e) => {
                self.eval(e)?;
                Ok(None)
            }
            Stmt::Break => Ok(Some(Signal::Break)),
            Stmt::Continue => Ok(Some(Signal::Continue)),
        }
    }

    fn do_assign(&mut self, target: &Expr, op: &AssignOp, rval: Value) -> Result<(), String> {
        if let Expr::Ident(name) = target {
            let new = match op {
                AssignOp::Set => rval,
                AssignOp::Add => {
                    let c = self.get_var(name).unwrap_or(Value::Null);
                    add_val(&c, &rval)
                }
                AssignOp::Sub => {
                    let c = self.get_var(name).unwrap_or(Value::Null);
                    sub_val(&c, &rval)
                }
                AssignOp::Mul => {
                    let c = self.get_var(name).unwrap_or(Value::Null);
                    mul_val(&c, &rval)
                }
                AssignOp::Div => {
                    let c = self.get_var(name).unwrap_or(Value::Null);
                    div_val(&c, &rval)
                }
            };
            self.set_var(name, new);
            return Ok(());
        }
        if let Expr::Index { obj, idx } = target {
            if let Expr::Ident(name) = obj.as_ref() {
                let i = self.eval(idx)?.as_int() as usize;
                if let Some(Value::List(mut v)) = self.get_var(name) {
                    if i < v.len() {
                        v[i] = rval;
                        self.set_var(name, Value::List(v));
                    }
                }
            }
            return Ok(());
        }
        if let Expr::Member { obj, field } = target {
            if let Expr::Ident(name) = obj.as_ref() {
                if let Some(Value::Map(mut m)) = self.get_var(name) {
                    m.insert(field.clone(), rval);
                    self.set_var(name, Value::Map(m));
                }
            }
            return Ok(());
        }
        Err("代入先が不正".into())
    }
}

fn add_val(l: &Value, r: &Value) -> Value {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
        (Value::Str(a), _) => Value::Str(format!("{}{}", a, r)),
        _ => Value::Float(l.as_float() + r.as_float()),
    }
}
fn sub_val(l: &Value, r: &Value) -> Value {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
        _ => Value::Float(l.as_float() - r.as_float()),
    }
}
fn mul_val(l: &Value, r: &Value) -> Value {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
        _ => Value::Float(l.as_float() * r.as_float()),
    }
}
fn div_val(l: &Value, r: &Value) -> Value {
    let d = r.as_float();
    if d == 0.0 {
        return Value::Null;
    }
    match (l, r) {
        (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a / b),
        _ => Value::Float(l.as_float() / d),
    }
}

// シンプルな高速乱数（外部依存なし）
fn fast_rand() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static S: AtomicU64 = AtomicU64::new(123456789);
    let s = S.fetch_add(0x9e3779b97f4a7c15, Ordering::Relaxed);
    let mut x = s ^ (s >> 30);
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

// ── ゲームループスレッドのエントリポイント ────────────────────
pub fn run_game(stmts: Vec<Stmt>, state: GameState, target_fps: u64) {
    let mut interp = Interpreter::new(state.clone_arcs());

    // トップレベル文を実行（FuncDef, VarDecl など）
    if let Err(e) = interp.exec_stmts(&stmts) {
        eprintln!("[Mistral] トップレベルエラー: {}", e);
        state.running.store(false, Ordering::Relaxed);
        return;
    }

    // ready() — draw.background など bg_color を変更する呼び出しに対応
    interp.frame_cmds.clear();
    if let Some(func) = interp.get_var("ready") {
        if let Err(e) = interp.call_value(func, vec![]) {
            eprintln!("[Mistral] ready() エラー: {}", e);
        }
    }
    // ready() 内で draw.background が呼ばれていたら bg_color へ即反映
    // （draw() ループ前なので swap は不要、bg_color だけ更新すれば十分）
    for cmd in &interp.frame_cmds {
        if let DrawCmd::Background(col) = cmd {
            *state.bg_color.lock().unwrap() = *col;
        }
    }
    interp.frame_cmds.clear();

    // frame_cmds のバッファを事前確保（ゲーム中のヒープ再アロケを防止）
    interp.frame_cmds.reserve(128);

    let frame_time = std::time::Duration::from_nanos(1_000_000_000 / target_fps);
    let mut last = std::time::Instant::now();

    while state.running.load(Ordering::Relaxed) {
        let now = std::time::Instant::now();
        let delta = now.duration_since(last).as_secs_f64();
        last = now;

        // update(delta)
        if let Some(func) = interp.get_var("update") {
            if let Err(e) = interp.call_value(func, vec![Value::Float(delta)]) {
                eprintln!("[Mistral] update() エラー: {}", e);
                break;
            }
        }

        // draw() — ローカルバッファに蓄積してから一括スワップ
        interp.frame_cmds.clear();
        if let Some(func) = interp.get_var("draw") {
            if let Err(e) = interp.call_value(func, vec![]) {
                eprintln!("[Mistral] draw() エラー: {}", e);
                break;
            }
        }
        if let Ok(mut q) = state.draw_cmds.lock() {
            std::mem::swap(&mut *q, &mut interp.frame_cmds);
        }

        // ── デバッグ計測（リリースビルドでは完全除去） ──
        #[cfg(debug_assertions)]
        {
            let frame_us = now.elapsed().as_micros();
            if frame_us > 20_000 {
                eprintln!("[VM-SPIKE] frame={}us | cmds={}", frame_us, interp.frame_cmds.len());
            }
        }

        // フレーム同期: fixed timestep
        let remaining = frame_time.checked_sub(last.elapsed());
        if let Some(rem) = remaining {
            if rem > std::time::Duration::from_millis(2) {
                std::thread::sleep(rem - std::time::Duration::from_millis(2));
            }
            while last.elapsed() < frame_time {
                std::hint::spin_loop();
            }
        }
    }
}


// ── HEX カラー文字列パーサー ──────────────────────────────────
// 対応フォーマット（# は省略可）:
//   #RGB        → #RRGGBBFF に展開
//   #RGBA       → #RRGGBBAA に展開
//   #RRGGBB     → アルファ = FF
//   #RRGGBBAA
fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let s = s.trim().trim_start_matches('#');
    let hex2f = |h: &str| -> Option<f32> {
        u8::from_str_radix(h, 16).ok().map(|b| b as f32 / 255.0)
    };
    match s.len() {
        3 => {
            // #RGB → #RRGGBB
            let r = hex2f(&s[0..1].repeat(2))?;
            let g = hex2f(&s[1..2].repeat(2))?;
            let b = hex2f(&s[2..3].repeat(2))?;
            Some([r, g, b, 1.0])
        }
        4 => {
            // #RGBA → #RRGGBBAA
            let r = hex2f(&s[0..1].repeat(2))?;
            let g = hex2f(&s[1..2].repeat(2))?;
            let b = hex2f(&s[2..3].repeat(2))?;
            let a = hex2f(&s[3..4].repeat(2))?;
            Some([r, g, b, a])
        }
        6 => {
            let r = hex2f(&s[0..2])?;
            let g = hex2f(&s[2..4])?;
            let b = hex2f(&s[4..6])?;
            Some([r, g, b, 1.0])
        }
        8 => {
            let r = hex2f(&s[0..2])?;
            let g = hex2f(&s[2..4])?;
            let b = hex2f(&s[4..6])?;
            let a = hex2f(&s[6..8])?;
            Some([r, g, b, a])
        }
        _ => None,
    }
}

