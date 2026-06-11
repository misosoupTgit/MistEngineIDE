pub mod gameloop;
pub mod renderer;
pub mod input;
pub mod collider;
pub mod button;
pub mod vm;        // DrawCmd / GameState などの共有型を保持
pub mod js_vm;     // QuickJS JavaScript ランタイム（Mistral の後継）
pub mod sdl_window;
