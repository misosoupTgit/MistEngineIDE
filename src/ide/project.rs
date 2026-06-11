/// プロジェクト管理
/// project.mist.json の読み書きと新規プロジェクト作成

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// anti_alias フィールドが JSON にない場合のデフォルト値（1.0 = オフ）
fn default_anti_alias() -> f32 { 2.0 }
fn default_high_dpi()    -> bool { true  }
fn default_vsync()      -> bool { true  }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub window_width: u32,
    pub window_height: u32,
    pub resizable: bool,
    /// true = DPI倍率で内部バッファを拡大しダウンサンプル（サイズは変わらず画質向上）
    #[serde(default = "default_high_dpi")]
    pub high_dpi: bool,
    /// SSAA倍率 (1.0=解析的AAのみ, 2.0=2×SSAA, 4.0=4×SSAA)
    #[serde(default = "default_anti_alias")]
    pub anti_alias: f32,
    /// true = 60fps上限 / false = 無制限
    #[serde(default = "default_vsync")]
    pub vsync: bool,
    pub main_file: String,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        ProjectConfig {
            name: "MyGame".to_string(),
            version: "0.1.0".to_string(),
            window_width: 1280,
            window_height: 720,
            resizable: true,
            high_dpi: true,
            anti_alias: 2.0,
            vsync: true,
            main_file: "main.js".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProjectEntry {
    pub config: ProjectConfig,
    pub path: PathBuf,
}

impl ProjectEntry {
    pub fn load(project_dir: &Path) -> Option<Self> {
        let config_path = project_dir.join("project.json");

        // 自動マイグレーション処理: project.json が存在せず project.mist.json がある場合移行する
        if !config_path.exists() {
            let old_config_path = project_dir.join("project.mist.json");
            if old_config_path.exists() {
                if let Ok(mut content) = std::fs::read_to_string(&old_config_path) {
                    content = content.replace("main.mist", "main.js");
                    if std::fs::write(&config_path, content).is_ok() {
                        let _ = std::fs::remove_file(old_config_path);
                    }
                }
            }
        }

        // main.mist から main.js への移行
        let old_main_path = project_dir.join("main.mist");
        let new_main_path = project_dir.join("main.js");
        if old_main_path.exists() && !new_main_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&old_main_path) {
                if std::fs::write(&new_main_path, content).is_ok() {
                    let _ = std::fs::remove_file(old_main_path);
                }
            }
        }

        let content = std::fs::read_to_string(&config_path).ok()?;
        let config: ProjectConfig = serde_json::from_str(&content).ok()?;
        Some(ProjectEntry {
            config,
            path: project_dir.to_path_buf(),
        })
    }

    pub fn save(&self) -> std::io::Result<()> {
        let config_path = self.path.join("project.json");
        let content = serde_json::to_string_pretty(&self.config)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(config_path, content)
    }
}

/// 新規プロジェクト作成パラメータ
#[derive(Debug, Clone)]
pub struct NewProjectParams {
    pub name: String,
    pub path: PathBuf,
    pub window_width: u32,
    pub window_height: u32,
    pub resizable: bool,
    pub high_dpi: bool,
    pub anti_alias: f32,
    pub vsync: bool,
}

impl Default for NewProjectParams {
    fn default() -> Self {
        NewProjectParams {
            name: "MyGame".to_string(),
            path: PathBuf::new(),
            window_width: 1280,
            window_height: 720,
            resizable: true,
            high_dpi: true,
            anti_alias: 2.0,
            vsync: true,
        }
    }
}

/// 新規プロジェクトを作成する
pub fn create_project(params: &NewProjectParams) -> std::io::Result<ProjectEntry> {
    let project_dir = params.path.join(&params.name);
    std::fs::create_dir_all(&project_dir)?;
    std::fs::create_dir_all(project_dir.join("assets"))?;

    // main.js
    let main_content = r#"// main.js - MistEngine エントリポイント

let player_x = 100.0;
let player_y = 100.0;
const speed = 200.0;

function ready() {
    print("Game started!");
}

function update(delta) {
    if (input.action_held("move_right")) {
        player_x += speed * delta;
    }
    if (input.action_held("move_left")) {
        player_x -= speed * delta;
    }
    if (input.action_held("move_down")) {
        player_y += speed * delta;
    }
    if (input.action_held("move_up")) {
        player_y -= speed * delta;
    }
}

function draw() {
    draw.circle(player_x, player_y, 32, Color.RED);
}
"#;
    std::fs::write(project_dir.join("main.js"), main_content)?;

    // input.json
    let input_content = r#"{
  "keys": {
    "move_up":    ["Key.W", "Key.Up",    "Controller.DPad.Up"],
    "move_down":  ["Key.S", "Key.Down",  "Controller.DPad.Down"],
    "move_left":  ["Key.A", "Key.Left",  "Controller.DPad.Left"],
    "move_right": ["Key.D", "Key.Right", "Controller.DPad.Right"],
    "jump":       ["Key.Space",          "Controller.A"],
    "attack":     ["Key.Z",              "Controller.X"],
    "pause":      ["Key.Escape",         "Controller.Start"]
  }
}
"#;
    std::fs::write(project_dir.join("input.json"), input_content)?;

    // project.json
    let config = ProjectConfig {
        name: params.name.clone(),
        version: "0.1.0".to_string(),
        window_width: params.window_width,
        window_height: params.window_height,
        resizable: params.resizable,
        high_dpi: params.high_dpi,
        anti_alias: params.anti_alias,
        vsync: params.vsync,
        main_file: "main.js".to_string(),
    };
    let config_content = serde_json::to_string_pretty(&config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(project_dir.join("project.json"), config_content)?;

    Ok(ProjectEntry { config, path: project_dir })
}

/// ファイルシステムからプロジェクト一覧をスキャン
pub fn scan_projects(search_dir: &Path) -> Vec<ProjectEntry> {
    let mut projects = Vec::new();
    if let Ok(entries) = std::fs::read_dir(search_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(proj) = ProjectEntry::load(&entry.path()) {
                    projects.push(proj);
                }
            }
        }
    }
    projects
}
