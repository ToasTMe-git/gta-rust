# GTA VII — Rust Edition

A native, high-FPS rewrite of the browser game, built with **Rust + wgpu**. Now in first-person with blocky Minecraft-style view arms.

## Run

```bash
cd /home/harel/gta7-rust
cargo run --release
```

On Linux systems without a usable GPU driver you can force software Vulkan:

```bash
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json cargo run --release
```

Click the window once to capture the mouse; press `ESC` to release it.

## Controls

| Key | Action |
|---|---|
| `W A S D` | Walk / drive |
| `SHIFT` | Sprint |
| `Mouse` | Look / aim (horizontal inverted, vertical normal; toggle `MOUSE_INV_X/Y` in `src/game.rs`) |
| `LMB` | Fire (hold — no spam clicking) |
| `1 / 2 / 3` | Pistol / Shotgun / SMG |
| `Q` / `RT` | Cycle weapon next |
| `LT` | Cycle weapon previous |
| `R` | Reload |
| `E` | Enter / exit vehicle |
| `SPACE` | Jump / handbrake |
| `R` | Reload |
| `LMB` | Lock cursor (also locked at startup) |
| `L` | Toggle mouse lock on/off |
| `M` | Generate a brand-new random map |

## Why it is fast

- **One draw call for the entire 3D scene.** Every cube (roads, buildings, cars, pedestrians, particles, markers) is an instance in a single vertex buffer, rendered with one instanced indexed draw.
- **One draw call for the sky** (fullscreen triangle, procedural gradient/sun/stars shader).
- **One draw call for the HUD.** The HUD is drawn into a 640×360 CPU framebuffer with a tiny bitmap font, uploaded as a single texture.
- No per-frame heap allocations in the hot path, no scene graph, no heavy ECS.
- WGSL shaders do lighting, fog and texture-atlas sampling on the GPU.
- Release profile uses LTO, single codegen unit, panic=abort and stripping.

The HUD shows a live FPS counter.

## Project layout

- `src/main.rs` — winit event loop and frame orchestration
- `src/engine.rs` — wgpu device, pipelines, instance buffer, texture atlas, render pass
- `src/game.rs` — world generation, player/pedestrian/car AI, weapons, wanted system, missions
- `src/hud.rs` — CPU framebuffer, bitmap font, bars and minimap
- `src/shader.wgsl` — scene and sky shaders
- `src/hud.wgsl` — HUD compositing shader
# gta-rust
