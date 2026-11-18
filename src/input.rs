use std::collections::HashSet;
use winit::event::{ElementState, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

pub struct Input {
    keys: HashSet<KeyCode>,
    just: HashSet<KeyCode>,
    pub left: bool,
    pub right: bool,
    pub left_just: bool,
    pub right_just: bool,
    pub dx: f32,
    pub dy: f32,
    pub grabbed: bool,
}

impl Input {
    pub fn new() -> Self {
        Self {
            keys: HashSet::new(),
            just: HashSet::new(),
            left: false,
            right: false,
            left_just: false,
            right_just: false,
            dx: 0.0,
            dy: 0.0,
            grabbed: false,
        }
    }

    pub fn key(&mut self, key: PhysicalKey, state: ElementState) {
        if let PhysicalKey::Code(code) = key {
            match state {
                ElementState::Pressed => {
                    if self.keys.insert(code) {
                        self.just.insert(code);
                    }
                }
                ElementState::Released => {
                    self.keys.remove(&code);
                }
            }
        }
    }

    pub fn mouse(&mut self, button: MouseButton, state: ElementState) {
        let down = state == ElementState::Pressed;
        match button {
            MouseButton::Left => {
                if down && !self.left { self.left_just = true; }
                self.left = down;
                if down { self.grabbed = true; }
            }
            MouseButton::Right => {
                if down && !self.right { self.right_just = true; }
                self.right = down;
            }
            _ => {}
        }
    }

    pub fn motion(&mut self, delta: (f64, f64)) {
        self.dx += delta.0 as f32;
        self.dy += delta.1 as f32;
    }

    pub fn down(&self, code: KeyCode) -> bool { self.keys.contains(&code) }
    pub fn pressed(&self, code: KeyCode) -> bool { self.just.contains(&code) }

    pub fn clear(&mut self) {
        self.keys.clear();
        self.just.clear();
        self.left = false;
        self.right = false;
        self.left_just = false;
        self.right_just = false;
        self.dx = 0.0;
        self.dy = 0.0;
        self.grabbed = false;
    }

    pub fn end_frame(&mut self) {
        self.just.clear();
        self.left_just = false;
        self.right_just = false;
        self.dx = 0.0;
        self.dy = 0.0;
    }
}
