/// 入力システム
/// input.jsonで定義されたアクションマップとXInput対応

use std::collections::{HashMap, HashSet};
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    pub keys: HashMap<String, Vec<String>>,
}

impl InputConfig {
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }

    pub fn default_config() -> Self {
        let mut keys = HashMap::new();
        keys.insert("move_up".to_string(),    vec!["Key.W".to_string(), "Key.Up".to_string()]);
        keys.insert("move_down".to_string(),  vec!["Key.S".to_string(), "Key.Down".to_string()]);
        keys.insert("move_left".to_string(),  vec!["Key.A".to_string(), "Key.Left".to_string()]);
        keys.insert("move_right".to_string(), vec!["Key.D".to_string(), "Key.Right".to_string()]);
        keys.insert("jump".to_string(),       vec!["Key.Space".to_string()]);
        keys.insert("attack".to_string(),     vec!["Key.Z".to_string()]);
        keys.insert("pause".to_string(),      vec!["Key.Escape".to_string()]);
        InputConfig { keys }
    }
}

/// 仮想キーコード（プラットフォーム非依存）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VirtualKey {
    // アルファベット
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    // 数字
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    // 特殊キー
    Space, Enter, Escape, Tab, Shift, Ctrl, Alt,
    Up, Down, Left, Right,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    // コントローラ（XInput）
    ControllerA, ControllerB, ControllerX, ControllerY,
    ControllerLB, ControllerRB, ControllerLT, ControllerRT,
    ControllerStart, ControllerBack,
    ControllerDPadUp, ControllerDPadDown, ControllerDPadLeft, ControllerDPadRight,
    ControllerLeftStickUp, ControllerLeftStickDown,
    ControllerLeftStickLeft, ControllerLeftStickRight,
    Unknown,
}

impl VirtualKey {
    pub fn from_str(s: &str) -> VirtualKey {
        match s {
            "Key.A" => VirtualKey::A, "Key.B" => VirtualKey::B,
            "Key.C" => VirtualKey::C, "Key.D" => VirtualKey::D,
            "Key.E" => VirtualKey::E, "Key.F" => VirtualKey::F,
            "Key.G" => VirtualKey::G, "Key.H" => VirtualKey::H,
            "Key.I" => VirtualKey::I, "Key.J" => VirtualKey::J,
            "Key.K" => VirtualKey::K, "Key.L" => VirtualKey::L,
            "Key.M" => VirtualKey::M, "Key.N" => VirtualKey::N,
            "Key.O" => VirtualKey::O, "Key.P" => VirtualKey::P,
            "Key.Q" => VirtualKey::Q, "Key.R" => VirtualKey::R,
            "Key.S" => VirtualKey::S, "Key.T" => VirtualKey::T,
            "Key.U" => VirtualKey::U, "Key.V" => VirtualKey::V,
            "Key.W" => VirtualKey::W, "Key.X" => VirtualKey::X,
            "Key.Y" => VirtualKey::Y, "Key.Z" => VirtualKey::Z,
            "Key.Space"  => VirtualKey::Space,
            "Key.Enter"  => VirtualKey::Enter,
            "Key.Escape" => VirtualKey::Escape,
            "Key.Tab"    => VirtualKey::Tab,
            "Key.Shift"  => VirtualKey::Shift,
            "Key.Ctrl"   => VirtualKey::Ctrl,
            "Key.Alt"    => VirtualKey::Alt,
            "Key.Up"     => VirtualKey::Up,
            "Key.Down"   => VirtualKey::Down,
            "Key.Left"   => VirtualKey::Left,
            "Key.Right"  => VirtualKey::Right,
            "Controller.A"      => VirtualKey::ControllerA,
            "Controller.B"      => VirtualKey::ControllerB,
            "Controller.X"      => VirtualKey::ControllerX,
            "Controller.Y"      => VirtualKey::ControllerY,
            "Controller.LB"     => VirtualKey::ControllerLB,
            "Controller.RB"     => VirtualKey::ControllerRB,
            "Controller.LT"     => VirtualKey::ControllerLT,
            "Controller.RT"     => VirtualKey::ControllerRT,
            "Controller.Start"  => VirtualKey::ControllerStart,
            "Controller.Back"   => VirtualKey::ControllerBack,
            "Controller.DPad.Up"    => VirtualKey::ControllerDPadUp,
            "Controller.DPad.Down"  => VirtualKey::ControllerDPadDown,
            "Controller.DPad.Left"  => VirtualKey::ControllerDPadLeft,
            "Controller.DPad.Right" => VirtualKey::ControllerDPadRight,
            _ => VirtualKey::Unknown,
        }
    }
}

/// 入力マネージャー
pub struct InputManager {
    config: InputConfig,
    /// アクション名 → バインドされたキーセット
    action_map: HashMap<String, Vec<VirtualKey>>,
    /// 現在フレームで押されているキー
    held_keys: HashSet<VirtualKey>,
    /// このフレームで新たに押されたキー
    pressed_keys: HashSet<VirtualKey>,
    /// このフレームで離されたキー
    released_keys: HashSet<VirtualKey>,
    prev_held: HashSet<VirtualKey>,
}

impl InputManager {
    pub fn new(config: InputConfig) -> Self {
        let mut action_map: HashMap<String, Vec<VirtualKey>> = HashMap::new();
        for (action, bindings) in &config.keys {
            let keys: Vec<VirtualKey> = bindings.iter()
                .map(|s| VirtualKey::from_str(s))
                .collect();
            action_map.insert(action.clone(), keys);
        }
        InputManager {
            config,
            action_map,
            held_keys: HashSet::new(),
            pressed_keys: HashSet::new(),
            released_keys: HashSet::new(),
            prev_held: HashSet::new(),
        }
    }

    /// フレーム開始時にpressed/releasedを更新
    pub fn begin_frame(&mut self) {
        self.pressed_keys = self.held_keys.difference(&self.prev_held).cloned().collect();
        self.released_keys = self.prev_held.difference(&self.held_keys).cloned().collect();
        self.prev_held = self.held_keys.clone();
    }

    pub fn key_down(&mut self, key: VirtualKey) {
        self.held_keys.insert(key);
    }

    pub fn key_up(&mut self, key: VirtualKey) {
        self.held_keys.remove(&key);
    }

    // ──────────────────────────────
    // Mistral input.* API
    // ──────────────────────────────

    /// アクションが押し続けられているか
    pub fn action_held(&self, action: &str) -> bool {
        self.action_map.get(action)
            .map(|keys| keys.iter().any(|k| self.held_keys.contains(k)))
            .unwrap_or(false)
    }

    /// アクションがこのフレームに押されたか
    pub fn action_pressed(&self, action: &str) -> bool {
        self.action_map.get(action)
            .map(|keys| keys.iter().any(|k| self.pressed_keys.contains(k)))
            .unwrap_or(false)
    }

    /// アクションがこのフレームに離されたか
    pub fn action_released(&self, action: &str) -> bool {
        self.action_map.get(action)
            .map(|keys| keys.iter().any(|k| self.released_keys.contains(k)))
            .unwrap_or(false)
    }

    pub fn key_held(&self, key: &VirtualKey) -> bool {
        self.held_keys.contains(key)
    }

    pub fn key_pressed(&self, key: &VirtualKey) -> bool {
        self.pressed_keys.contains(key)
    }

    pub fn key_released(&self, key: &VirtualKey) -> bool {
        self.released_keys.contains(key)
    }
}
