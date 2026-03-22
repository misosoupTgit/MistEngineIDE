/// ゲームループ
/// Mistralの ready() / update(delta) / draw() に対応

use std::time::{Instant, Duration};

pub struct GameLoop {
    pub running: bool,
    pub target_fps: u32,
    last_frame: Instant,
    pub delta: f64,
    pub total_time: f64,
    pub frame_count: u64,
}

impl GameLoop {
    pub fn new(fps: u32) -> Self {
        GameLoop {
            running: false,
            target_fps: fps,
            last_frame: Instant::now(),
            delta: 0.0,
            total_time: 0.0,
            frame_count: 0,
        }
    }

    /// フレーム開始。delta timeを返す
    pub fn tick(&mut self) -> f64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame);
        self.delta = elapsed.as_secs_f64();
        self.total_time += self.delta;
        self.frame_count += 1;
        self.last_frame = now;
        self.delta
    }

    /// 目標FPSに合わせてスリープ
    pub fn sleep_to_target(&self) {
        let target_duration = Duration::from_secs_f64(1.0 / self.target_fps as f64);
        let elapsed = self.last_frame.elapsed();
        if elapsed < target_duration {
            std::thread::sleep(target_duration - elapsed);
        }
    }

    pub fn fps(&self) -> f64 {
        if self.delta > 0.0 { 1.0 / self.delta } else { 0.0 }
    }

    pub fn start(&mut self) {
        self.running = true;
        self.last_frame = Instant::now();
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
}
