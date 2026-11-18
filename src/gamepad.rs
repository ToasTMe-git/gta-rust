// Minimal Linux gamepad reader using evdev directly (no libudev required).
use evdev::{
    AbsoluteAxisType, Device, EventType, FFEffect, FFEffectData, FFEffectKind, FFEffectType,
    FFReplay, InputEventKind, Key,
};
use libc::{fcntl, F_SETFL, O_NONBLOCK};
use std::fs;
use std::os::fd::AsRawFd;

fn deadzone(v: f32) -> f32 {
    if v.abs() < 0.18 { 0.0 } else { (v - v.signum() * 0.18) / 0.82 }
}

pub struct Gamepad {
    devices: Vec<Device>,
    effects: Vec<FFEffect>,
    pub move_x: f32,
    pub move_y: f32,
    pub look_x: f32,
    pub look_y: f32,
    pub fire: bool,
    pub sprint: bool,
    jump_prev: bool,
    enter_prev: bool,
    lt_prev: bool,
    rt_prev: bool,
    lt_now: bool,
    rt_now: bool,
    jump_now: bool,
    enter_now: bool,
}

impl Gamepad {
    pub fn new() -> Self {
        let mut devices = Vec::new();
        let mut effects = Vec::new();
        if let Ok(entries) = fs::read_dir("/dev/input") {
            for e in entries.flatten() {
                let path = e.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with("event") { continue; }
                if let Ok(mut dev) = Device::open(&path) {
                    let has_abs = dev.supported_events().contains(EventType::ABSOLUTE);
                    let has_btn = dev.supported_events().contains(EventType::KEY);
                    let axes = dev.supported_absolute_axes();
                    let stick = axes.map_or(false, |a| a.contains(AbsoluteAxisType::ABS_X));
                    if has_abs && has_btn && stick {
                        unsafe { fcntl(dev.as_raw_fd(), F_SETFL, O_NONBLOCK); }
                        let has_rumble = dev.supported_ff()
                            .map_or(false, |f| f.contains(FFEffectType::FF_RUMBLE));
                        if has_rumble {
                            let data = FFEffectData {
                                direction: 0,
                                trigger: Default::default(),
                                replay: FFReplay { length: 120, delay: 0 },
                                kind: FFEffectKind::Rumble { strong_magnitude: 0, weak_magnitude: 0 },
                            };
                            if let Ok(eff) = dev.upload_ff_effect(data) {
                                effects.push(eff);
                            }
                        }
                        devices.push(dev);
                    }
                }
            }
        }
        Self {
            devices,
            effects,
            move_x: 0.0, move_y: 0.0, look_x: 0.0, look_y: 0.0,
            fire: false, sprint: false,
            jump_prev: false, enter_prev: false, jump_now: false, enter_now: false,
            lt_prev: false, rt_prev: false, lt_now: false, rt_now: false,
        }
    }

    pub fn poll(&mut self) {
        let mut fire = false;
        let mut sprint = false;
        let mut lt = false;
        let mut rt = false;
        let mut jump = false;
        let mut enter = false;

        for dev in &mut self.devices {
            let events = match dev.fetch_events() { Ok(e) => e, Err(_) => continue };
            for ev in events {
                match ev.kind() {
                    InputEventKind::AbsAxis(ax) => {
                        let v = deadzone(ev.value() as f32 / 32767.0);
                        match ax {
                            AbsoluteAxisType::ABS_X => self.move_x = v,
                            AbsoluteAxisType::ABS_Y => self.move_y = v,
                            AbsoluteAxisType::ABS_Z => lt = ev.value() > 128,
                            AbsoluteAxisType::ABS_RZ => rt = ev.value() > 128,
                            AbsoluteAxisType::ABS_RX => self.look_x = v,
                            AbsoluteAxisType::ABS_RY => self.look_y = v,
                            _ => {}
                        }
                    }
                    InputEventKind::Key(k) => match k {
                        Key::BTN_SOUTH | Key::BTN_WEST => fire = true,
                        Key::BTN_EAST => jump = true,
                        Key::BTN_NORTH => enter = true,
                        Key::BTN_THUMBL | Key::BTN_THUMBR | Key::BTN_TRIGGER => sprint = true,
                        Key::BTN_TL => lt = true,
                        Key::BTN_TR => rt = true,
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        self.jump_now = jump && !self.jump_prev;
        self.enter_now = enter && !self.enter_prev;
        self.jump_prev = jump;
        self.enter_prev = enter;
        self.lt_now = lt && !self.lt_prev;
        self.rt_now = rt && !self.rt_prev;
        self.lt_prev = lt;
        self.rt_prev = rt;
        self.fire = fire;
        self.sprint = sprint;
    }

    pub fn jump_pressed(&self) -> bool { self.jump_now }
    pub fn enter_pressed(&self) -> bool { self.enter_now }
    pub fn rumble(&mut self, strong: u16, weak: u16, ms: u16) {
        for eff in &mut self.effects {
            let data = FFEffectData {
                direction: 0,
                trigger: Default::default(),
                replay: FFReplay { length: ms, delay: 0 },
                kind: FFEffectKind::Rumble {
                    strong_magnitude: strong.min(65535),
                    weak_magnitude: weak.min(65535),
                },
            };
            let _ = eff.update(data);
            let _ = eff.play(1);
        }
    }

    pub fn lt_pressed(&self) -> bool { self.lt_now }
    pub fn rt_pressed(&self) -> bool { self.rt_now }
}
