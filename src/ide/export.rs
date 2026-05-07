/// MistEngine エクスポートモジュール
///
/// 自己埋め込み方式でスタンドアロン exe を生成する。
///
/// # バイナリフォーマット（末尾から逆読みできる設計）
/// ```
/// [mistengine.exe バイト列 ... ]   ← 純粋な PE 実行ファイル
/// [script UTF-8 bytes          ]   ← スクリプト本体
/// [config UTF-8 JSON bytes     ]   ← 設定 JSON
/// [u32 LE: script_len          ]   ← スクリプトのバイト数（4 bytes）
/// [u32 LE: config_len          ]   ← JSON のバイト数（4 bytes）
/// [8 bytes: b"MISTGAME"        ]   ← マジックバイト
/// ```
///
/// 読み取り時は末尾 16 バイトを読むだけで両サイズが確定する。
/// スクリプトや設定の中に MISTGAME バイト列が偶然現れても誤検知しない。

use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// マジックバイト（8 バイト固定）
pub const EMBED_MAGIC: &[u8; 8] = b"MISTGAME";

// ── エラー型 ──────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ExportError {
    Io(std::io::Error),
    ScriptNotFound(PathBuf),
    SelfExeNotFound,
    SizeMismatch { expected: u64, actual: u64 },
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ExportError::Io(e)                     => write!(f, "IO エラー: {}", e),
            ExportError::ScriptNotFound(p)         => write!(f, "スクリプトが見つかりません: {}", p.display()),
            ExportError::SelfExeNotFound           => write!(f, "実行ファイルのパスを取得できません"),
            ExportError::SizeMismatch { expected, actual } =>
                write!(f, "書き込みサイズ不一致: 期待 {} bytes, 実際 {} bytes", expected, actual),
        }
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self { ExportError::Io(e) }
}

// ── エクスポートオプション ────────────────────────────────────────

pub struct ExportOptions {
    pub project_dir: PathBuf,
    pub main_file:   String,
    pub title:       String,
    pub width:       u32,
    pub height:      u32,
    pub resizable:   bool,
    pub high_dpi:    bool,
    pub anti_alias:  f32,
    pub vsync:       bool,
    pub output_path: PathBuf,
}

// ── 埋め込みデータ ────────────────────────────────────────────────

pub struct EmbeddedData {
    pub script:      String,
    pub config_json: String,
}

// ── 起動時検出 ────────────────────────────────────────────────────

/// 起動した exe に埋め込みデータがあれば取り出す。
/// なければ None（通常の IDE モードで起動）。
pub fn try_read_embedded() -> Option<EmbeddedData> {
    let exe_path = std::env::current_exe().ok()?;
    let bytes    = std::fs::read(&exe_path).ok()?;
    parse_embedded(&bytes)
}

/// バイト列から埋め込みデータをパースする（テスト可能な純粋関数）
///
/// フォーマット末尾:
/// ... [script][config][script_len: u32 LE][config_len: u32 LE][MAGIC: 8 bytes]
///                      ↑ -16 bytes from end              ↑ -8 bytes from end
fn parse_embedded(bytes: &[u8]) -> Option<EmbeddedData> {
    // 最小サイズ: magic(8) + config_len(4) + script_len(4) + config(1) + script(1) = 18
    if bytes.len() < 18 { return None; }

    // ① 末尾 8 バイトがマジックか確認
    let len = bytes.len();
    if &bytes[len - 8..] != EMBED_MAGIC {
        return None;
    }

    // ② 末尾から config_len (bytes[len-12..len-8]) と script_len (bytes[len-16..len-12]) を読む
    if len < 16 { return None; }
    let config_len  = u32_le(&bytes[len - 12..len -  8]) as usize;
    let script_len  = u32_le(&bytes[len - 16..len - 12]) as usize;

    // ③ サイズ検証
    let trailer_len = script_len + config_len + 4 + 4 + 8; // script + config + 2×u32 + magic
    if len < trailer_len { return None; }

    let exe_end     = len - trailer_len;
    let script_end  = exe_end + script_len;
    let config_end  = script_end + config_len;

    let script      = std::str::from_utf8(&bytes[exe_end..script_end]).ok()?.to_string();
    let config_json = std::str::from_utf8(&bytes[script_end..config_end]).ok()?.to_string();

    eprintln!("[embed] 埋め込み検出 script={}B config={}B", script_len, config_len);
    Some(EmbeddedData { script, config_json })
}

#[inline]
fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

// ── エクスポート ──────────────────────────────────────────────────

/// Mistral プロジェクトをスタンドアロン exe としてエクスポートする。
///
/// 生成された exe を実行すると IDE は起動せず、埋め込みスクリプトを
/// 直接ゲームウィンドウで実行する。Cargo / Rustc は不要。
///
/// # 返り値
/// 成功時は書き込んだバイト数を返す。
pub fn export_exe(opts: &ExportOptions) -> Result<u64, ExportError> {
    // ── 1. 現在の exe を読み込む ──────────────────────────────────
    let self_path = std::env::current_exe()
        .map_err(|_| ExportError::SelfExeNotFound)?;
    eprintln!("[export] 自己exe: {}", self_path.display());

    let self_bytes = std::fs::read(&self_path)?;
    eprintln!("[export] 自己exeサイズ: {} bytes", self_bytes.len());

    // 既に埋め込み済みの場合は純粋な exe 部分だけを使う
    let exe_end   = find_exe_end(&self_bytes);
    let exe_bytes = &self_bytes[..exe_end];
    eprintln!("[export] 書き込み exe サイズ: {} bytes", exe_bytes.len());

    // ── 2. スクリプトを読み込む ──────────────────────────────────
    let script_path = opts.project_dir.join(&opts.main_file);
    if !script_path.exists() {
        return Err(ExportError::ScriptNotFound(script_path));
    }
    let script = std::fs::read_to_string(&script_path)?;
    eprintln!("[export] スクリプト: {} chars", script.len());

    // ── 3. 設定 JSON を構築 ──────────────────────────────────────
    let config_json = build_config_json(opts);
    eprintln!("[export] config JSON: {} chars", config_json.len());

    // ── 4. 出力ディレクトリを作成 ────────────────────────────────
    let out_path = opts.output_path.clone();
    eprintln!("[export] 出力先: {}", out_path.display());

    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // ── 5. BufWriter で書き込み ───────────────────────────────────
    // フォーマット: [exe][script][config][script_len: 4 bytes][config_len: 4 bytes][MAGIC: 8 bytes]
    let script_bytes = script.as_bytes();
    let config_bytes = config_json.as_bytes();
    let expected_size = (exe_bytes.len()
        + script_bytes.len()
        + config_bytes.len()
        + 4 + 4 + 8) as u64;

    {
        let file = std::fs::File::create(&out_path)?;
        let mut out = BufWriter::new(file);

        out.write_all(exe_bytes)?;         // ① exe 本体
        out.write_all(script_bytes)?;      // ② スクリプト本文
        out.write_all(config_bytes)?;      // ③ 設定 JSON 本文
        out.write_all(&(script_bytes.len() as u32).to_le_bytes())?;  // ④ script_len (4 bytes)
        out.write_all(&(config_bytes.len() as u32).to_le_bytes())?;  // ⑤ config_len (4 bytes)
        out.write_all(EMBED_MAGIC)?;       // ⑥ マジック (8 bytes)
        out.flush()?;
    }

    // ── 6. サイズ検証 ────────────────────────────────────────────
    let actual_size = std::fs::metadata(&out_path)?.len();
    eprintln!("[export] 書き込み完了: {} bytes (期待: {})", actual_size, expected_size);

    if actual_size != expected_size {
        return Err(ExportError::SizeMismatch {
            expected: expected_size,
            actual:   actual_size,
        });
    }

    Ok(actual_size)
}

fn build_config_json(opts: &ExportOptions) -> String {
    serde_json::json!({
        "name":          opts.title,
        "version":       "1.0.0",
        "window_width":  opts.width,
        "window_height": opts.height,
        "resizable":     opts.resizable,
        "high_dpi":      opts.high_dpi,
        "anti_alias":    opts.anti_alias,
        "vsync":         opts.vsync,
        "main_file":     opts.main_file,
    }).to_string()
}

/// 既存の自己埋め込みデータを取り除いた exe 末尾インデックスを返す
fn find_exe_end(bytes: &[u8]) -> usize {
    let len = bytes.len();
    if len < 18 { return len; }
    if &bytes[len - 8..] != EMBED_MAGIC { return len; } // 埋め込みなし

    let config_len = u32_le(&bytes[len - 12..len -  8]) as usize;
    let script_len = u32_le(&bytes[len - 16..len - 12]) as usize;
    let trailer    = script_len + config_len + 4 + 4 + 8;

    if len < trailer { return len; }
    len - trailer
}

// ── ヘルパー ──────────────────────────────────────────────────────

/// エクスポートした exe の埋め込み設定を取得する（デバッグ用）
pub fn read_config_from_embedded(
    data: &EmbeddedData,
) -> Option<crate::ide::project::ProjectConfig> {
    serde_json::from_str(&data.config_json).ok()
}

/// デフォルトのエクスポート先パスを取得する（ユーザーデスクトップ）
pub fn default_export_path(game_name: &str) -> String {
    let safe_name = game_name.replace(' ', "_")
                             .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let desktop = std::env::var("USERPROFILE")
        .map(|p| format!("{}\\Desktop", p))
        .or_else(|_| std::env::var("HOME").map(|p| format!("{}/Desktop", p)))
        .unwrap_or_else(|_| ".".to_string());
    format!("{}\\{}.exe", desktop, safe_name)
}
