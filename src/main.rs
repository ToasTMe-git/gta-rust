mod audio;
mod engine;
mod game;
mod gamepad;
mod hud;
mod input;
mod save;

use std::time::Instant;
use winit::dpi::PhysicalPosition;
use winit::event::{DeviceEvent, ElementState, Event, MouseButton, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowBuilder};

use engine::Engine;
use gamepad::Gamepad;
use game::Game;
use input::Input;

// grab modes: 0 = Locked, 1 = Confined, 2 = manual centering, 3 = unlocked
fn lock_cursor(window: &Window, mode: &mut u8) {
    if window.set_cursor_grab(CursorGrabMode::Locked).is_ok() {
        *mode = 0;
    } else if window.set_cursor_grab(CursorGrabMode::Confined).is_ok() {
        *mode = 1;
    } else {
        *mode = 2; // fallback: keep warping the cursor to the window center
    }
    let _ = window.set_cursor_visible(false);
}

fn unlock_cursor(window: &Window, mode: &mut u8) {
    let _ = window.set_cursor_grab(CursorGrabMode::None);
    let _ = window.set_cursor_visible(true);
    *mode = 3;
}

fn center_cursor(window: &Window) {
    let s = window.inner_size();
    let _ = window.set_cursor_position(PhysicalPosition::new(s.width / 2, s.height / 2));
}

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let window = WindowBuilder::new()
        .with_title("GTA VII — Rust Edition")
        .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720))
        .build(&event_loop)
        .expect("failed to create window");

    let mut engine = pollster::block_on(async { Engine::new(&window) });
    let mut game = Game::new();
    let mut input = Input::new();
    let mut pad = Gamepad::new();
    let mut last = Instant::now();
    let mut frames = 0u32;
    let mut grab_mode = 0u8;
    let mut lock_enabled = true;

    lock_cursor(&window, &mut grab_mode);
    input.grabbed = true;

    println!("GTA VII ready — L toggles mouse lock.");

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => elwt.exit(),
                    WindowEvent::Focused(false) => input.clear(),
                    WindowEvent::Focused(true) => {
                        if lock_enabled {
                            lock_cursor(&window, &mut grab_mode);
                            input.grabbed = true;
                        }
                    }
                    WindowEvent::Resized(size) => engine.resize(size.width, size.height),
                    WindowEvent::KeyboardInput { event, .. } => {
                        if let PhysicalKey::Code(code) = event.physical_key {
                            if code == KeyCode::KeyL && event.state == ElementState::Pressed {
                                lock_enabled = !lock_enabled;
                                if lock_enabled {
                                    lock_cursor(&window, &mut grab_mode);
                                    input.grabbed = true;
                                } else {
                                    unlock_cursor(&window, &mut grab_mode);
                                    input.grabbed = false;
                                }
                            }
                        }
                        input.key(event.physical_key, event.state);
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        input.mouse(button, state);
                        if state == ElementState::Pressed && button == MouseButton::Left && lock_enabled {
                            lock_cursor(&window, &mut grab_mode);
                            input.grabbed = true;
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        // Manual fallback: use cursor delta from screen center as look input.
                        if lock_enabled && grab_mode == 2 {
                            let s = window.inner_size();
                            let dx = position.x - s.width as f64 / 2.0;
                            let dy = position.y - s.height as f64 / 2.0;
                            input.motion((dx, dy));
                            center_cursor(&window);
                        }
                    }
                    WindowEvent::CursorLeft { .. } => {
                        if lock_enabled && grab_mode == 2 { center_cursor(&window); }
                    }
                    WindowEvent::RedrawRequested => {
                        let now = Instant::now();
                        let dt = (now - last).as_secs_f32().min(0.05);
                        last = now;

                        let (w, h) = engine.size();
                        let aspect = w as f32 / h.max(1) as f32;
                        pad.poll();
                        game.update(dt, &input, aspect, &pad);

                        // Controller rumble feedback.
                        if game.rumble_fire > 0 {
                            let n = game.rumble_fire as u32;
                            pad.rumble((n * 5000).min(65535) as u16, (n * 9000).min(65535) as u16, 90);
                            game.rumble_fire = 0;
                        }
                        if game.rumble_hit > 0 {
                            let n = game.rumble_hit as u32;
                            pad.rumble((n * 12000).min(65535) as u16, (n * 18000).min(65535) as u16, 220);
                            game.rumble_hit = 0;
                        }

                        engine.instances.clear();
                        game.collect(&mut engine.instances);
                        let globals = game.globals(w, h);
                        let show_menu = !game.started;
                        let hud = game.hud_pixels();
                        engine.render(globals, hud, show_menu);

                        frames += 1;
                        if frames % 30 == 0 && lock_enabled {
                            lock_cursor(&window, &mut grab_mode);
                            input.grabbed = true;
                            if grab_mode == 2 { center_cursor(&window); }
                        }

                        input.end_frame();
                        window.request_redraw();
                    }
                    _ => {}
                },
                Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                    if lock_enabled && grab_mode != 2 { input.motion(delta); }
                }
                _ => {}
            }
        })
        .unwrap();
}
