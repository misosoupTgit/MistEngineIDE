/// プロジェクト管理
/// project.mist.json の読み書きと新規プロジェクト作成

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub window_width: u32,
    pub window_height: u32,
    pub resizable: bool,
    pub high_dpi: bool,
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
            main_file: "main.mist".to_string(),
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
        let config_path = project_dir.join("project.mist.json");
        let content = std::fs::read_to_string(&config_path).ok()?;
        let config: ProjectConfig = serde_json::from_str(&content).ok()?;
        Some(ProjectEntry {
            config,
            path: project_dir.to_path_buf(),
        })
    }

    pub fn save(&self) -> std::io::Result<()> {
        let config_path = self.path.join("project.mist.json");
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
        }
    }
}

/// 新規プロジェクトを作成する
pub fn create_project(params: &NewProjectParams) -> std::io::Result<ProjectEntry> {
    let project_dir = params.path.join(&params.name);
    std::fs::create_dir_all(&project_dir)?;
    std::fs::create_dir_all(project_dir.join("assets"))?;

    // main.mist
    let main_content = r#"// main.mist - MistEngine エントリポイント

let player_x: float = 100.0
let player_y: float = 100.0
let speed: float = 200.0

func ready() {
    print("Game started!")
}

func update(delta) {
    if input.action_held("move_right") {
        player_x += speed * delta
    }
    if input.action_held("move_left") {
        player_x -= speed * delta
    }
    if input.action_held("move_down") {
        player_y += speed * delta
    }
    if input.action_held("move_up") {
        player_y -= speed * delta
    }
}

func draw() {
    draw.circle(player_x, player_y, 32, color=Color.RED)
}
"#;
    std::fs::write(project_dir.join("main.mist"), main_content)?;

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

    // project.mist.json
    let config = ProjectConfig {
        name: params.name.clone(),
        version: "0.1.0".to_string(),
        window_width: params.window_width,
        window_height: params.window_height,
        resizable: params.resizable,
        high_dpi: params.high_dpi,
        main_file: "main.mist".to_string(),
    };
    let config_content = serde_json::to_string_pretty(&config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(project_dir.join("project.mist.json"), config_content)?;

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
