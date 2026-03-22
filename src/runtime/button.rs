/// ボタンシステム
/// 図形・画像オブジェクトをそのままボタンデザインとして使用

use crate::runtime::collider::AABB;

/// ボタンイベント
#[derive(Debug, Clone, PartialEq)]
pub enum ButtonEvent {
    None,
    Hover,
    Click,
    Release,
}

/// ボタン状態
#[derive(Debug, Clone, PartialEq)]
pub enum ButtonState {
    Normal,
    Hovered,
    Pressed,
}

pub struct Button {
    pub id: u64,
    pub aabb: AABB,
    pub state: ButtonState,
    pub on_click:   Option<Box<dyn Fn()>>,
    pub on_hover:   Option<Box<dyn Fn()>>,
    pub on_release: Option<Box<dyn Fn()>>,
    pub enabled: bool,
}

impl Button {
    pub fn new(id: u64, aabb: AABB) -> Self {
        Button {
            id,
            aabb,
            state: ButtonState::Normal,
            on_click: None,
            on_hover: None,
            on_release: None,
            enabled: true,
        }
    }

    pub fn on_click<F: Fn() + 'static>(mut self, f: F) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }

    pub fn on_hover<F: Fn() + 'static>(mut self, f: F) -> Self {
        self.on_hover = Some(Box::new(f));
        self
    }

    pub fn on_release<F: Fn() + 'static>(mut self, f: F) -> Self {
        self.on_release = Some(Box::new(f));
        self
    }

    /// フレームごとにマウス状態を渡して更新
    pub fn update(&mut self, mouse_x: f32, mouse_y: f32, mouse_pressed: bool, mouse_released: bool) -> ButtonEvent {
        if !self.enabled { return ButtonEvent::None; }

        let hovered = self.aabb.contains_point(mouse_x, mouse_y);
        let prev_state = self.state.clone();

        self.state = match (&prev_state, hovered, mouse_pressed, mouse_released) {
            (_, true,  true,  _    ) => ButtonState::Pressed,
            (ButtonState::Pressed, true, _, true) => {
                if let Some(f) = &self.on_click { f(); }
                ButtonState::Hovered
            }
            (ButtonState::Pressed, false, _, true) => ButtonState::Normal,
            (_, true,  false, false) => ButtonState::Hovered,
            _ => ButtonState::Normal,
        };

        match (&prev_state, &self.state) {
            (ButtonState::Normal, ButtonState::Hovered) => {
                if let Some(f) = &self.on_hover { f(); }
                ButtonEvent::Hover
            }
            (ButtonState::Pressed, ButtonState::Hovered) => ButtonEvent::Click,
            (ButtonState::Hovered | ButtonState::Pressed, ButtonState::Normal) => {
                if let Some(f) = &self.on_release { f(); }
                ButtonEvent::Release
            }
            _ => ButtonEvent::None,
        }
    }
}

pub struct ButtonManager {
    buttons: Vec<Button>,
    next_id: u64,
}

impl ButtonManager {
    pub fn new() -> Self {
        ButtonManager { buttons: Vec::new(), next_id: 0 }
    }

    pub fn add(&mut self, aabb: AABB) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.buttons.push(Button::new(id, aabb));
        id
    }

    pub fn update_all(&mut self, mx: f32, my: f32, pressed: bool, released: bool) {
        for btn in &mut self.buttons {
            btn.update(mx, my, pressed, released);
        }
    }

    pub fn get(&self, id: u64) -> Option<&Button> {
        self.buttons.iter().find(|b| b.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Button> {
        self.buttons.iter_mut().find(|b| b.id == id)
    }
}
