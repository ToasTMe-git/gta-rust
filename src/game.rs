use glam::{Mat4, Vec2, Vec3};
use std::f32::consts::PI;
use winit::keyboard::KeyCode;

use crate::audio;
use crate::engine::{Globals, Instance, HUD_W, HUD_H};
use crate::gamepad::Gamepad;
use crate::save;
use crate::hud::{self, Fb};
use crate::input::Input;

const ROAD_W: f32 = 18.0;
// Mouse look polarity. Both are flipped by request; change to 1.0 to reverse an axis.
const MOUSE_INV_X: f32 = -1.0;
const MOUSE_INV_Y: f32 = 1.0;

struct Rng(u64);
impl Rng {
    fn with_seed(seed: u64) -> Self { Self(if seed == 0 { 1 } else { seed }) }
    fn u(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as u32
    }
    fn f(&mut self, a: f32, b: f32) -> f32 { a + (self.u() as f32 / u32::MAX as f32) * (b - a) }
    fn i(&mut self, a: i32, b: i32) -> i32 { a + (self.u() % (b - a + 1) as u32) as i32 }
    fn chance(&mut self, p: f32) -> bool { self.f(0.0, 1.0) < p }
}

fn clamp(v: f32, a: f32, b: f32) -> f32 { v.max(a).min(b) }
fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
fn ang_diff(a: f32, b: f32) -> f32 {
    ((b - a + PI) % (2.0 * PI) + 2.0 * PI) % (2.0 * PI) - PI
}

fn inst(pos: Vec3, size: Vec3, yaw: f32, color: [f32; 4], tex: f32, uv: (f32, f32), emis: f32) -> Instance {
    let m = Mat4::from_translation(pos) * Mat4::from_rotation_y(yaw) * Mat4::from_scale(size);
    Instance {
        model: m.to_cols_array_2d(),
        color,
        params: [uv.0, uv.1, tex, emis],
    }
}

fn blocked(buildings: &[Building], x: f32, z: f32, r: f32) -> bool {
    buildings.iter().any(|b| x + r > b.min_x && x - r < b.max_x && z + r > b.min_z && z - r < b.max_z)
}

fn fire_into(
    bullets: &mut Vec<Bullet>, particles: &mut Vec<Particle>, shake: &mut f32, rng: &mut Rng,
    origin: Vec3, dir: Vec3, owner: u8, dmg: f32, speed: f32,
) {
    bullets.push(Bullet { pos: origin, vel: dir * speed, life: 1.5, owner, dmg });
    let muzzle = origin + dir * 0.5;
    particles.push(Particle {
        pos: muzzle, vel: Vec3::ZERO, life: 0.06, max: 0.06, size: 0.42,
        color: [1.0, 0.96, 0.7, 1.0],
    });
    for _ in 0..7 {
        let v = Vec3::new(rng.f(-1.0, 1.0), rng.f(-0.3, 0.8), rng.f(-1.0, 1.0)) * rng.f(2.0, 6.0);
        particles.push(Particle {
            pos: muzzle, vel: v, life: rng.f(0.08, 0.18), max: 0.18, size: rng.f(0.08, 0.18),
            color: [1.0, 0.78, 0.3, 1.0],
        });
    }
    *shake = shake.max(0.16);
}

fn boom_into(particles: &mut Vec<Particle>, shake: &mut f32, rng: &mut Rng, pos: Vec3) {
    for _ in 0..22 {
        let v = Vec3::new(rng.f(-1.0, 1.0), rng.f(0.0, 1.4), rng.f(-1.0, 1.0)) * rng.f(3.0, 12.0);
        let c = if rng.chance(0.5) { [1.0, 0.6, 0.15, 1.0] } else { [0.3, 0.3, 0.32, 0.7] };
        particles.push(Particle { pos, vel: v, life: rng.f(0.4, 1.4), max: 1.4, size: rng.f(0.3, 1.0), color: c });
    }
    *shake = shake.max(1.2);
}

#[derive(Clone, Copy)]
struct WeaponSpec {
    name: &'static str,
    mag: i32,
    reserve: i32,
    dmg: f32,
    rate: f32,
    pellets: u32,
    spread: f32,
    speed: f32,
    recoil: f32,
    reload_time: f32,
}

const WEAPONS: [WeaponSpec; 3] = [
    WeaponSpec { name: "PISTOL", mag: 12, reserve: 120, dmg: 26.0, rate: 0.16, pellets: 1, spread: 0.012, speed: 110.0, recoil: 0.045, reload_time: 1.1 },
    WeaponSpec { name: "SHOTGUN", mag: 6, reserve: 48, dmg: 14.0, rate: 0.72, pellets: 8, spread: 0.09, speed: 90.0, recoil: 0.16, reload_time: 1.6 },
    WeaponSpec { name: "SMG", mag: 30, reserve: 180, dmg: 16.0, rate: 0.075, pellets: 1, spread: 0.035, speed: 120.0, recoil: 0.03, reload_time: 1.3 },
];

#[derive(Clone, Copy)]
struct Building { min_x: f32, max_x: f32, min_z: f32, max_z: f32 }

#[derive(Clone)]
struct Car {
    pos: Vec3,
    yaw: f32,
    speed: f32,
    health: f32,
    is_police: bool,
    traffic: bool,
    target_speed: f32,
    color: [f32; 4],
    siren: f32,
}

#[derive(Clone)]
struct Ped {
    pos: Vec3,
    yaw: f32,
    target_yaw: f32,
    speed: f32,
    health: f32,
    dead: bool,
    dead_time: f32,
    is_police: bool,
    fire_cd: f32,
    flee: f32,
    target: bool,
    timer: f32,
    walk: f32,
    color: [f32; 4],
}

struct Bullet {
    pos: Vec3,
    vel: Vec3,
    life: f32,
    owner: u8,
    dmg: f32,
}

struct Particle {
    pos: Vec3,
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    color: [f32; 4],
}

struct Player {
    pos: Vec3,
    yaw: f32,
    vy: f32,
    grounded: bool,
    health: f32,
    armor: f32,
    wep: usize,
    mags: [i32; 3],
    reserves: [i32; 3],
    wanted: i32,
    heat: f32,
    wanted_timer: f32,
    money: i32,
    car: Option<usize>,
    fire_cd: f32,
    reload: f32,
    stolen: i32,
    walk: f32,
    dead_timer: f32,
    hit_cd: f32,
    stun: f32,
    enter_cd: f32,
}

impl Player {
    fn new() -> Self {
        Self {
            pos: Vec3::new(30.0, 0.0, 30.0), yaw: 0.0, vy: 0.0, grounded: true,
            health: 100.0, armor: 50.0,
            wep: 0, mags: [WEAPONS[0].mag, WEAPONS[1].mag, WEAPONS[2].mag],
            reserves: [WEAPONS[0].reserve, WEAPONS[1].reserve, WEAPONS[2].reserve],
            wanted: 0, heat: 0.0,
            wanted_timer: 0.0, money: 250, car: None, fire_cd: 0.0, reload: 0.0,
            stolen: 0, walk: 0.0, dead_timer: 0.0, hit_cd: 0.0, stun: 0.0, enter_cd: 0.0,
        }
    }
    fn spec(&self) -> &'static WeaponSpec { &WEAPONS[self.wep] }
    fn ammo(&self) -> i32 { self.mags[self.wep] }
    fn reserve(&self) -> i32 { self.reserves[self.wep] }
    fn switch_weapon(&mut self, i: usize) {
        if i < 3 && i != self.wep {
            self.wep = i;
            self.reload = 0.0;
            self.fire_cd = self.fire_cd.max(0.18);
        }
    }

    fn add_wanted(&mut self, n: f32) {
        self.heat += n;
        while self.heat >= 1.0 && self.wanted < 5 {
            self.heat -= 1.0;
            self.wanted += 1;
        }
        if self.wanted >= 5 { self.heat = self.heat.min(0.99); }
        self.wanted_timer = 0.0;
    }
    fn hurt(&mut self, amount: f32) {
        if self.dead_timer > 0.0 { return; }
        let mut a = amount;
        if self.armor > 0.0 {
            let abs = self.armor.min(a * 0.6);
            self.armor -= abs;
            a -= abs;
        }
        self.health -= a;
        if self.health <= 0.0 {
            self.health = 0.0;
            self.dead_timer = 1.5;
            audio::play("death", 400);
            self.car = None; // never stay trapped in a vehicle after death
            self.hit_cd = 0.0;
            self.stun = 0.0;
            self.money = (self.money - 200).max(0);
        }
    }
    fn try_hurt(&mut self, amount: f32) -> bool {
        if self.dead_timer > 0.0 || self.hit_cd > 0.0 { return false; }
        self.hurt(amount);
        audio::play("hit", 90);
        self.hit_cd = 0.75;
        self.stun = 0.18;
        true
    }
}

struct Mission {
    stage: u8, // 0 = goto, 1 = kill, 2 = return
    dest: Vec2,
    home: Vec2,
    reward: i32,
    timer: f32,
    targets: Vec<usize>,
}

pub struct Game {
    pub started: bool,
    rng: Rng,
    map_seed: u64,
    roads: Vec<f32>,
    player: Player,
    peds: Vec<Ped>,
    cars: Vec<Car>,
    bullets: Vec<Bullet>,
    particles: Vec<Particle>,
    buildings: Vec<Building>,
    statics: Vec<Instance>,
    pub cam_yaw: f32,
    cam_pitch: f32,
    shake: f32,
    gun_recoil: f32,
    hit_marker: f32,
    respawn_flash: f32,
    pub rumble_fire: u8,
    pub rumble_hit: u8,
    time: f32,
    day_time: f32,
    mission: Mission,
    toast: String,
    toast_time: f32,
    police_timer: f32,
    fps: f32,
    hud: Fb,
    cam_vp: Mat4,
    cam_inv: Mat4,
    cam_pos: Vec3,
}

fn fire_player_weapon(
    player: &mut Player,
    bullets: &mut Vec<Bullet>,
    particles: &mut Vec<Particle>,
    shake: &mut f32,
    rng: &mut Rng,
    toast: &mut String,
    toast_time: &mut f32,
    gun_recoil: &mut f32,
    rumble_fire: &mut u8,
    origin: Vec3,
    dir: Vec3,
    in_car: bool,
) {
    if player.reload > 0.0 || player.fire_cd > 0.0 { return; }
    let w = player.wep;
    let spec = WEAPONS[w];
    if player.mags[w] <= 0 {
        if player.reserves[w] > 0 {
            player.reload = spec.reload_time;
        } else {
            *toast = "OUT OF AMMO".to_string();
            *toast_time = 1.0;
        }
        player.fire_cd = 0.25;
        return;
    }
    player.mags[w] -= 1;
    player.fire_cd = spec.rate;
    audio::play(match w { 0 => "pistol", 1 => "shotgun", _ => "smg" }, 45);
    for _ in 0..spec.pellets {
        let spread = Vec3::new(
            rng.f(-spec.spread, spec.spread),
            rng.f(-spec.spread, spec.spread),
            rng.f(-spec.spread, spec.spread),
        );
        let d = (dir + spread).normalize();
        let o = if spec.pellets > 1 {
            origin + Vec3::new(rng.f(-0.15, 0.15), rng.f(-0.1, 0.1), rng.f(-0.15, 0.15))
        } else { origin };
        fire_into(bullets, particles, shake, rng, o, d, 0, spec.dmg, spec.speed);
    }
    *gun_recoil = (*gun_recoil + spec.recoil * if in_car { 0.6 } else { 1.0 }).min(0.3);
    player.add_wanted(0.25);
    *rumble_fire = rumble_fire.saturating_add(1).min(3);
}

fn generate_roads(rng: &mut Rng) -> Vec<f32> {
    let count = rng.i(5, 7) as usize;
    let base = 300.0 / (count - 1) as f32;
    let mut roads = vec![-150.0];
    for i in 1..count - 1 {
        roads.push(-150.0 + base * i as f32 + rng.f(-9.0, 9.0));
    }
    roads.push(150.0);
    roads.sort_by(|a, b| a.partial_cmp(b).unwrap());
    roads
}

impl Game {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345);
        let mut temp_rng = Rng::with_seed(seed);
        let roads = generate_roads(&mut temp_rng);
        let mut g = Self {
            rng: Rng::with_seed(seed),
            map_seed: seed,
            roads,
            started: false,
            player: Player::new(),
            peds: Vec::new(),
            cars: Vec::new(),
            bullets: Vec::new(),
            particles: Vec::new(),
            buildings: Vec::new(),
            statics: Vec::new(),
            cam_yaw: 0.0,
            cam_pitch: 0.30,
            shake: 0.0,
            gun_recoil: 0.0,
            hit_marker: 0.0,
            respawn_flash: 0.0,
            rumble_fire: 0,
            rumble_hit: 0,
            time: 0.0,
            day_time: 12.0,
            mission: Mission { stage: 0, dest: Vec2::new(30.0, 30.0), home: Vec2::new(30.0, 30.0), reward: 500, timer: 5.0, targets: Vec::new() },
            toast: String::new(),
            toast_time: 0.0,
            police_timer: 3.0,
            fps: 60.0,
            hud: Fb::new(HUD_W, HUD_H),
            cam_vp: Mat4::IDENTITY,
            cam_inv: Mat4::IDENTITY,
            cam_pos: Vec3::ZERO,
        };
        g.build_world();
        g.player.pos = g.spawn_point();
        g.spawn_peds(18);
        g.spawn_cars(12);
        g
    }

    fn road_point(&mut self) -> (Vec2, f32) {
        let line = self.roads[self.rng.i(0, (self.roads.len() - 1) as i32) as usize];
        if self.rng.chance(0.5) {
            (Vec2::new(line + self.rng.f(-2.0, 2.0), self.rng.f(-140.0, 140.0)), PI / 2.0)
        } else {
            (Vec2::new(self.rng.f(-140.0, 140.0), line + self.rng.f(-2.0, 2.0)), 0.0)
        }
    }

    fn free_point(&mut self) -> Vec2 {
        for _ in 0..40 {
            let p = Vec2::new(self.rng.f(-140.0, 140.0), self.rng.f(-140.0, 140.0));
            if !blocked(&self.buildings, p.x, p.y, 1.5) { return p; }
        }
        Vec2::ZERO
    }

    fn spawn_point(&self) -> Vec3 {
        for &x in &self.roads {
            for &z in &self.roads {
                if !blocked(&self.buildings, x, z, 3.0) {
                    return Vec3::new(x, 0.0, z);
                }
            }
        }
        Vec3::new(self.roads[0], 0.0, self.roads[0])
    }

    fn build_world(&mut self) {
        let s = &mut self.statics;
        let b = &mut self.buildings;
        let rng = &mut self.rng;

        s.push(inst(Vec3::new(0.0, -0.6, 0.0), Vec3::new(1700.0, 1.0, 1700.0), 0.0, [1.0, 1.0, 1.0, 1.0], 2.0, (100.0, 100.0), 0.0));

        for &v in &self.roads {
            s.push(inst(Vec3::new(v, 0.0, 0.0), Vec3::new(ROAD_W, 0.12, 320.0), 0.0, [1.0, 1.0, 1.0, 1.0], 0.0, (2.0, 16.0), 0.0));
            s.push(inst(Vec3::new(0.0, 0.0, v), Vec3::new(320.0, 0.12, ROAD_W), 0.0, [1.0, 1.0, 1.0, 1.0], 0.0, (16.0, 2.0), 0.0));
            for side in [-1.0f32, 1.0] {
                let off = ROAD_W / 2.0 + 3.6;
                s.push(inst(Vec3::new(v + side * off, 0.02, 0.0), Vec3::new(7.2, 0.18, 320.0), 0.0, [1.0, 1.0, 1.0, 1.0], 1.0, (2.0, 16.0), 0.0));
                s.push(inst(Vec3::new(0.0, 0.02, v + side * off), Vec3::new(320.0, 0.18, 7.2), 0.0, [1.0, 1.0, 1.0, 1.0], 1.0, (16.0, 2.0), 0.0));
            }
            for p in (-150..150).step_by(12) {
                s.push(inst(Vec3::new(v, 0.09, p as f32), Vec3::new(0.16, 0.03, 4.0), 0.0, [0.95, 0.88, 0.52, 1.0], -1.0, (1.0, 1.0), 0.0));
                s.push(inst(Vec3::new(p as f32, 0.09, v), Vec3::new(4.0, 0.03, 0.16), 0.0, [0.95, 0.88, 0.52, 1.0], -1.0, (1.0, 1.0), 0.0));
            }
        }

        // buildings: stacks of chunky Minecraft-style blocks
        const CB: f32 = 4.0;
        let palettes: [[f32; 3]; 4] = [
            [0.78, 0.80, 0.83], // stone
            [0.68, 0.42, 0.34], // brick
            [0.58, 0.41, 0.25], // wood
            [0.84, 0.78, 0.62], // sandstone
        ];
        for i in 0..self.roads.len() - 1 {
            for j in 0..self.roads.len() - 1 {
                let cx = (self.roads[i] + self.roads[i + 1]) / 2.0;
                let cz = (self.roads[j] + self.roads[j + 1]) / 2.0;
                if rng.chance(0.11) {
                    s.push(inst(Vec3::new(cx, 0.1, cz), Vec3::new(42.0, 0.2, 42.0), 0.0, [0.8, 1.0, 0.8, 1.0], 2.0, (4.0, 4.0), 0.0));
                    s.push(inst(Vec3::new(cx, 0.5, cz), Vec3::new(4.0, 0.9, 4.0), 0.0, [0.7, 0.72, 0.75, 1.0], -1.0, (1.0, 1.0), 0.0));
                    s.push(inst(Vec3::new(cx, 1.4, cz), Vec3::new(0.7, 1.8, 0.7), 0.0, [0.7, 0.72, 0.75, 1.0], -1.0, (1.0, 1.0), 0.0));
                    continue;
                }
                if (cx - 0.0).abs() < 1.0 && (cz - 60.0).abs() < 1.0 { continue; }
                if (cx + 60.0).abs() < 1.0 && (cz + 60.0).abs() < 1.0 { continue; }
                let lots = rng.i(1, 3);
                let spots = [(-11.0, -11.0), (11.0, -11.0), (-11.0, 11.0), (11.0, 11.0)];
                for k in 0..lots {
                    let (ox, oz) = spots[k as usize];
                    let w = rng.f(14.0, 19.0);
                    let d = rng.f(14.0, 19.0);
                    let h = rng.f(10.0, 38.0) * if rng.chance(0.15) { 1.5 } else { 1.0 };
                    let bx = cx + ox + rng.f(-2.0, 2.0);
                    let bz = cz + oz + rng.f(-2.0, 2.0);
                    let nx = (w / CB).round().max(2.0) as usize;
                    let nz = (d / CB).round().max(2.0) as usize;
                    let ny = (h / CB).round().max(2.0) as usize;
                    let pal = palettes[rng.i(0, 3) as usize];

                    for yy in 0..ny {
                        for xx in 0..nx {
                            for zz in 0..nz {
                                let shell = xx == 0 || xx == nx - 1 || zz == 0 || zz == nz - 1 || yy == 0 || yy == ny - 1;
                                if !shell { continue; }
                                let px = bx - (nx as f32 * CB) / 2.0 + CB * (xx as f32 + 0.5);
                                let pz = bz - (nz as f32 * CB) / 2.0 + CB * (zz as f32 + 0.5);
                                let py = CB * (yy as f32 + 0.5);
                                let wall = xx == 0 || xx == nx - 1 || zz == 0 || zz == nz - 1;
                                let window = wall && yy > 0 && yy < ny - 1 && rng.chance(0.16);
                                let shade = rng.f(0.86, 1.12);
                                let (col, tex, emis) = if window {
                                    ([0.10, 0.20, 0.33, 1.0], -1.0, 0.25)
                                } else {
                                    ([pal[0] * shade, pal[1] * shade, pal[2] * shade, 1.0], 3.0, 0.0)
                                };
                                s.push(inst(Vec3::new(px, py, pz), Vec3::new(CB * 0.98, CB * 0.98, CB * 0.98), 0.0, col, tex, (1.0, 1.0), emis));
                            }
                        }
                    }
                    b.push(Building {
                        min_x: bx - (nx as f32 * CB) / 2.0,
                        max_x: bx + (nx as f32 * CB) / 2.0,
                        min_z: bz - (nz as f32 * CB) / 2.0,
                        max_z: bz + (nz as f32 * CB) / 2.0,
                    });
                }
            }
        }

        // trees and lamps
        for &v in &self.roads {
            let mut p = -138.0;
            while p < 138.0 {
                let side = if rng.chance(0.5) { -1.0 } else { 1.0 };
                let off = ROAD_W / 2.0 + 3.6;
                if rng.chance(0.55) {
                    let tx = if rng.chance(0.5) { v + side * off } else { p };
                    let tz = if rng.chance(0.5) { p } else { v + side * off };
                    s.push(inst(Vec3::new(tx, 1.4, tz), Vec3::new(0.4, 2.8, 0.4), 0.0, [0.4, 0.27, 0.14, 1.0], -1.0, (1.0, 1.0), 0.0));
                    s.push(inst(Vec3::new(tx, 3.6, tz), Vec3::new(4.0, 2.0, 4.0), 0.0, [0.12, 0.42, 0.14, 1.0], -1.0, (1.0, 1.0), 0.0));
                    s.push(inst(Vec3::new(tx, 5.2, tz), Vec3::new(3.0, 2.0, 3.0), 0.0, [0.16, 0.50, 0.16, 1.0], -1.0, (1.0, 1.0), 0.0));
                    s.push(inst(Vec3::new(tx, 6.6, tz), Vec3::new(2.0, 1.0, 2.0), 0.0, [0.20, 0.56, 0.20, 1.0], -1.0, (1.0, 1.0), 0.0));
                }
                s.push(inst(Vec3::new(v + side * off, 3.1, p + 12.0), Vec3::new(0.18, 6.2, 0.18), 0.0, [0.24, 0.26, 0.29, 1.0], -1.0, (1.0, 1.0), 0.0));
                s.push(inst(Vec3::new(v + side * off - side * 1.5, 6.1, p + 12.0), Vec3::new(0.5, 0.35, 0.5), 0.0, [1.0, 0.9, 0.6, 1.0], -1.0, (1.0, 1.0), 0.7));
                p += 28.0;
            }
        }

        // beach strip + water
        s.push(inst(Vec3::new(0.0, -0.05, 172.0), Vec3::new(340.0, 0.1, 34.0), 0.0, [1.0, 1.0, 1.0, 1.0], 1.0, (20.0, 3.0), 0.0));
        s.push(inst(Vec3::new(0.0, -0.25, 420.0), Vec3::new(1600.0, 0.1, 500.0), 0.0, [1.0, 1.0, 1.0, 1.0], 4.0, (40.0, 20.0), 0.0));
        for x in (-150..150).step_by(18) {
            s.push(inst(Vec3::new(x as f32, 3.5, rng.f(160.0, 184.0)), Vec3::new(0.35, 7.0, 0.35), 0.0, [0.45, 0.32, 0.18, 1.0], -1.0, (1.0, 1.0), 0.0));
            for k in 0..5 {
                let a = (k as f32 / 5.0) * 2.0 * PI;
                s.push(inst(Vec3::new(x as f32 + a.cos() * 2.0, 6.6, 170.0 + a.sin() * 2.0), Vec3::new(3.2, 0.3, 1.0), a, [0.16, 0.5, 0.22, 1.0], -1.0, (1.0, 1.0), 0.0));
            }
        }
    }

    fn spawn_peds(&mut self, n: usize) {
        for _ in 0..n {
            let p = self.free_point();
            let shade = self.rng.f(0.5, 1.0);
            self.peds.push(Ped {
                pos: Vec3::new(p.x, 0.0, p.y), yaw: self.rng.f(0.0, 6.28), target_yaw: self.rng.f(0.0, 6.28),
                speed: self.rng.f(1.4, 2.6), health: 60.0, dead: false, dead_time: 0.0, is_police: false,
                fire_cd: 0.0, flee: 0.0, target: false, timer: self.rng.f(1.0, 4.0), walk: 0.0,
                color: [shade * self.rng.f(0.5, 1.0), shade * self.rng.f(0.4, 0.8), shade, 1.0],
            });
        }
    }

    fn spawn_cars(&mut self, n: usize) {
        for _ in 0..n {
            let (p, yaw) = loop {
                let q = self.road_point();
                let d = (q.0.x - self.player.pos.x).hypot(q.0.y - self.player.pos.z);
                if d > 10.0 { break q; }
            };
            self.cars.push(Car {
                pos: Vec3::new(p.x, 0.0, p.y), yaw, speed: 0.0, health: 100.0, is_police: false,
                traffic: true, target_speed: self.rng.f(-12.0, 12.0),
                color: [
                    self.rng.f(0.3, 1.0), self.rng.f(0.2, 0.8), self.rng.f(0.2, 0.9), 1.0,
                ],
                siren: 0.0,
            });
        }
        // A few parked cars so there is always something to steal without chasing traffic.
        for _ in 0..6 {
            let (p, yaw) = loop {
                let q = self.road_point();
                if (q.0.x - self.player.pos.x).hypot(q.0.y - self.player.pos.z) > 12.0 { break q; }
            };
            self.cars.push(Car {
                pos: Vec3::new(p.x, 0.0, p.y), yaw, speed: 0.0, health: 100.0, is_police: false,
                traffic: false, target_speed: 0.0, color: [0.7, 0.72, 0.75, 1.0], siren: 0.0,
            });
        }
    }

    fn spawn_police(&mut self) {
        let near = self.player.pos;
        for _ in 0..2 {
            let a = self.rng.f(0.0, 6.28);
            let r = self.rng.f(16.0, 26.0);
            let p = Vec2::new(near.x + a.cos() * r, near.z + a.sin() * r);
            let p = if blocked(&self.buildings, p.x, p.y, 1.0) { self.free_point() } else { p };
            self.peds.push(Ped {
                pos: Vec3::new(p.x, 0.0, p.y), yaw: a, target_yaw: a, speed: 5.0, health: 90.0,
                dead: false, dead_time: 0.0, is_police: true, fire_cd: self.rng.f(0.3, 1.0),
                flee: 0.0, target: false, timer: 0.0, walk: 0.0, color: [0.1, 0.2, 0.6, 1.0],
            });
        }
        let cop_cars = self.cars.iter().filter(|c| c.is_police && c.health > 0.0).count();
        if cop_cars < self.player.wanted as usize {
            let (p, yaw) = self.road_point();
            self.cars.push(Car {
                pos: Vec3::new(p.x, 0.0, p.y), yaw, speed: 0.0, health: 100.0, is_police: true,
                traffic: true, target_speed: 16.0, color: [0.92, 0.92, 0.95, 1.0], siren: 0.0,
            });
        }
    }

    pub fn update(&mut self, dt: f32, input: &Input, aspect: f32, pad: &Gamepad) {
        self.time += dt;
        if dt > 0.0 { self.fps = lerp(self.fps, 1.0 / dt, 0.05); }
        self.day_time = (self.day_time + dt * 0.025) % 24.0;
        if self.toast_time > 0.0 { self.toast_time -= dt; }
        if self.shake > 0.0 { self.shake = (self.shake - dt * 2.0).max(0.0); }
        if self.player.dead_timer > 0.0 && self.player.car.is_some() { self.player.car = None; }
        self.gun_recoil *= (-dt * 9.0).exp();
        if self.hit_marker > 0.0 { self.hit_marker -= dt; }
        if self.respawn_flash > 0.0 { self.respawn_flash -= dt; }

        if input.pressed(KeyCode::KeyM) { self.regenerate_map(); }
        if input.pressed(KeyCode::F5) { self.save_game(); }
        if input.pressed(KeyCode::F9) { self.load_game(); }

        if !self.started {
            if input.left_just
                || input.pressed(KeyCode::Space)
                || input.down(KeyCode::KeyW)
                || input.down(KeyCode::KeyA)
                || input.down(KeyCode::KeyS)
                || input.down(KeyCode::KeyD)
                || pad.enter_pressed()
                || pad.fire
            {
                self.started = true;
                self.say("WELCOME TO VICE CITY");
            }
            self.cam_yaw += dt * 0.06;
            self.update_camera(aspect, input, false);
            return;
        }

        self.cam_yaw += input.dx * 0.0025 * MOUSE_INV_X;
        self.cam_pitch = clamp(self.cam_pitch + input.dy * 0.002 * MOUSE_INV_Y, -0.3, 0.75);
        self.cam_yaw += pad.look_x * dt * 2.6 * MOUSE_INV_X;
        self.cam_pitch = clamp(self.cam_pitch + pad.look_y * dt * 1.7 * MOUSE_INV_Y, -0.3, 0.75);

        // wanted decay and police presence
        if self.player.dead_timer <= 0.0 {
            self.player.wanted_timer += dt;
            if self.player.wanted > 0 && self.player.wanted_timer > 11.0 {
                self.player.wanted -= 1;
                self.player.heat = (self.player.heat - 0.5).max(0.0);
                self.player.wanted_timer = 0.0;
            }
            self.police_timer -= dt;
            if self.player.wanted > 0 && self.police_timer <= 0.0 {
                self.police_timer = 4.0;
                let cops = self.peds.iter().filter(|p| p.is_police && !p.dead).count();
                if cops < self.player.wanted as usize * 2 { self.spawn_police(); }
            }
        }

        self.update_player(dt, input, pad);
        self.update_peds(dt);
        self.update_cars(dt);
        self.update_bullets(dt);
        self.update_particles(dt);
        self.update_mission(dt);
        self.update_camera(aspect, input, true);
    }

    fn rebuild_map_with_seed(&mut self, seed: u64) {
        self.map_seed = seed;
        self.rng = Rng::with_seed(seed);
        self.roads = generate_roads(&mut self.rng);
        self.statics.clear();
        self.buildings.clear();
        self.cars.clear();
        self.peds.clear();
        self.bullets.clear();
        self.particles.clear();
        self.player.car = None;
        self.player.pos = Vec3::new(self.roads[self.roads.len() / 2], 0.0, self.roads[self.roads.len() / 2]);
        self.player.health = 100.0;
        self.player.armor = 50.0;
        self.build_world();
        self.player.pos = self.spawn_point();
        self.spawn_peds(18);
        self.spawn_cars(12);
        self.mission.stage = 1;
        self.mission.timer = 3.0;
    }

    fn regenerate_map(&mut self) {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64 ^ (self.rng.u() as u64))
            .unwrap_or(1);
        self.rebuild_map_with_seed(seed);
        self.toast = format!("NEW MAP SEED {}", seed % 100000);
        self.toast_time = 2.5;
    }

    fn save_game(&mut self) {
        let data = save::SaveData {
            version: 1,
            seed: self.map_seed,
            px: self.player.pos.x,
            pz: self.player.pos.z,
            health: self.player.health,
            armor: self.player.armor,
            money: self.player.money,
            wanted: self.player.wanted,
            heat: self.player.heat,
            day: self.day_time,
            wep: self.player.wep as u8,
            mags: self.player.mags,
            reserves: self.player.reserves,
            mission_stage: self.mission.stage,
            mission_reward: self.mission.reward,
        };
        match save::save(&data) {
            Ok(_) => self.say("GAME SAVED (ENCRYPTED)"),
            Err(_) => self.say("SAVE FAILED"),
        }
    }

    fn load_game(&mut self) {
        match save::load() {
            Ok(d) => {
                self.rebuild_map_with_seed(d.seed);
                self.player.pos = Vec3::new(d.px, 0.0, d.pz);
                if blocked(&self.buildings, d.px, d.pz, 0.5) { self.player.pos = self.spawn_point(); }
                self.player.health = d.health;
                self.player.armor = d.armor;
                self.player.money = d.money;
                self.player.wanted = d.wanted;
                self.player.heat = d.heat;
                self.day_time = d.day;
                if (d.wep as usize) < 3 { self.player.wep = d.wep as usize; }
                self.player.mags = d.mags;
                self.player.reserves = d.reserves;
                self.mission.stage = d.mission_stage;
                self.mission.reward = d.mission_reward;
                self.say("GAME LOADED");
            }
            Err(_) => self.say("NO SAVE / LOAD FAILED"),
        }
    }

    fn explode_car_at(&mut self, pos: Vec3) {
        audio::play("explosion", 120);
        boom_into(&mut self.particles, &mut self.shake, &mut self.rng, pos);
        for p in &mut self.peds {
            if !p.dead && (p.pos.x - pos.x).powi(2) + (p.pos.z - pos.z).powi(2) < 64.0 {
                p.health -= 120.0;
                if p.health <= 0.0 { p.dead = true; p.dead_time = 0.0; }
            }
        }
        if self.player.car.is_none()
            && (self.player.pos.x - pos.x).powi(2) + (self.player.pos.z - pos.z).powi(2) < 64.0
            && self.player.try_hurt(32.0)
        {
            self.rumble_hit = self.rumble_hit.saturating_add(1).min(3);
        }
        self.player.add_wanted(1.5);
        self.toast = "CAR EXPLOSION".to_string();
        self.toast_time = 1.6;
    }

    fn say(&mut self, s: &str) {
        self.toast = s.to_string();
        self.toast_time = 2.4;
    }

    fn update_player(&mut self, dt: f32, input: &Input, pad: &Gamepad) {
        let respawn_pos = self.spawn_point();
        let pl = &mut self.player;
        if pl.dead_timer > 0.0 {
            pl.dead_timer -= dt;
            if pl.dead_timer <= 0.0 {
                pl.pos = respawn_pos;
                pl.health = 100.0; pl.armor = 50.0;
                pl.wep = 0;
                pl.mags = [WEAPONS[0].mag, WEAPONS[1].mag, WEAPONS[2].mag];
                pl.reserves = [WEAPONS[0].reserve, WEAPONS[1].reserve, WEAPONS[2].reserve];
                pl.wanted = 0; pl.heat = 0.0; pl.car = None;
                pl.vy = 0.0; pl.grounded = true; pl.stun = 0.0; pl.hit_cd = 0.0;
                pl.reload = 0.0; pl.fire_cd = 0.0; pl.yaw = 0.0; pl.enter_cd = 0.0;
                self.respawn_flash = 0.7;
                for c in &mut self.cars { if c.is_police { c.health = 0.0; } }
                self.cars.retain(|c| c.health > 0.0);
            }
            return;
        }

        if pl.enter_cd > 0.0 { pl.enter_cd -= dt; }
        if pl.hit_cd > 0.0 { pl.hit_cd -= dt; }
        if pl.stun > 0.0 { pl.stun -= dt; }

        if pl.fire_cd > 0.0 { pl.fire_cd -= dt; }
        if pl.reload > 0.0 {
            pl.reload -= dt;
            if pl.reload <= 0.0 {
                let w = pl.wep;
                let need = WEAPONS[w].mag - pl.mags[w];
                let take = need.min(pl.reserves[w]);
                pl.mags[w] += take;
                pl.reserves[w] -= take;
            }
        }

        if input.pressed(KeyCode::Digit1) { pl.switch_weapon(0); }
        if input.pressed(KeyCode::Digit2) { pl.switch_weapon(1); }
        if input.pressed(KeyCode::Digit3) { pl.switch_weapon(2); }
        if input.pressed(KeyCode::KeyQ) || pad.rt_pressed() { let n = (pl.wep + 1) % 3; pl.switch_weapon(n); }
        if pad.lt_pressed() { let n = if pl.wep == 0 { 2 } else { pl.wep - 1 }; pl.switch_weapon(n); }

        if let Some(ci) = pl.car {
            let c = &mut self.cars[ci];
            let steer = ((input.down(KeyCode::KeyD) as i32 - input.down(KeyCode::KeyA) as i32) as f32 + pad.move_x).clamp(-1.0, 1.0);
            let handbrake = input.down(KeyCode::Space) || pad.jump_pressed();
            let accel = ((input.down(KeyCode::KeyW) as i32 - input.down(KeyCode::KeyS) as i32) as f32 - pad.move_y).clamp(-1.0, 1.0);
            if accel > 0.0 { c.speed += 26.0 * dt; }
            else if accel < 0.0 { c.speed -= 30.0 * dt; }
            else { c.speed *= (1.0 - dt * 0.4).max(0.0); }
            if handbrake { c.speed *= (1.0 - dt * 5.0).max(0.0); }
            c.speed = c.speed.clamp(-16.0, 38.0);
            let grip = ((c.speed.abs() / 7.0).clamp(0.0, 1.0)).sqrt();
            // The look direction is the driving direction: mouse and A/D both steer.
            self.cam_yaw -= steer * dt * 2.6 * grip * if handbrake { 1.5 } else { 1.0 } * if c.speed >= 0.0 { 1.0 } else { -1.0 };
            c.yaw = self.cam_yaw;

            let nx = c.pos.x + c.yaw.sin() * c.speed * dt;
            let nz = c.pos.z + c.yaw.cos() * c.speed * dt;
            let hit_building = blocked(&self.buildings, nx, nz, 1.2);
            let out_of_bounds = nx.abs() > 165.0 || nz.abs() > 185.0;
            if hit_building && out_of_bounds {
                // Rare: both — just clamp and bounce.
                c.pos.x = c.pos.x.clamp(-160.0, 160.0);
                c.pos.z = c.pos.z.clamp(-160.0, 160.0);
                c.speed *= -0.12;
                self.shake = self.shake.max(0.25);
            } else if hit_building {
                // Slide along the wall instead of sticking to it.
                let okx = !blocked(&self.buildings, nx, c.pos.z, 1.2);
                let okz = !blocked(&self.buildings, c.pos.x, nz, 1.2);
                if okx { c.pos.x = nx; }
                if okz { c.pos.z = nz; }
                if !okx && !okz {
                    // Fully embedded: push out of the nearest face and nudge steering.
                    let r = 1.3;
                    for b in &self.buildings {
                        if nx + r > b.min_x && nx - r < b.max_x && nz + r > b.min_z && nz - r < b.max_z {
                            let dl = nx - b.min_x;
                            let dr = b.max_x - nx;
                            let db = nz - b.min_z;
                            let df = b.max_z - nz;
                            let m = dl.min(dr).min(db).min(df);
                            if m == dl { c.pos.x = b.min_x - r - 0.15; }
                            else if m == dr { c.pos.x = b.max_x + r + 0.15; }
                            else if m == db { c.pos.z = b.min_z - r - 0.15; }
                            else { c.pos.z = b.max_z + r + 0.15; }
                            c.yaw += steer.signum() * 0.3;
                            break;
                        }
                    }
                    c.speed *= -0.12;
                    self.shake = self.shake.max(0.3);
                } else {
                    c.speed *= 0.85;
                    self.shake = self.shake.max(0.15);
                }
            } else if out_of_bounds {
                c.pos.x = c.pos.x.clamp(-160.0, 160.0);
                c.pos.z = c.pos.z.clamp(-160.0, 160.0);
                c.speed *= -0.12;
                self.shake = self.shake.max(0.25);
            } else {
                c.pos.x = nx; c.pos.z = nz;
                for p in &mut self.peds {
                    if !p.dead && (p.pos.x - nx).powi(2) + (p.pos.z - nz).powi(2) < 4.0 && c.speed.abs() > 7.0 {
                        p.dead = true; p.dead_time = 0.0; p.health = 0.0;
                        pl.add_wanted(0.8);
                    }
                }
            }

            if input.left && input.grabbed || pad.fire {
                let p = self.cam_pitch.min(0.5);
                let dir = Vec3::new(self.cam_yaw.sin() * p.cos(), -p.sin() * 0.4, self.cam_yaw.cos() * p.cos()).normalize();
                let origin = c.pos + Vec3::new(0.0, 1.5, 0.0) + Vec3::new(c.yaw.sin(), 0.0, c.yaw.cos()) * 1.6;
                let Game { bullets, particles, shake, rng, toast, toast_time, gun_recoil, rumble_fire, .. } = self;
                fire_player_weapon(pl, bullets, particles, shake, rng, toast, toast_time, gun_recoil, rumble_fire, origin, dir, true);
            }
            if (input.pressed(KeyCode::KeyE) || pad.enter_pressed()) && pl.enter_cd <= 0.0 && c.speed.abs() < 8.0 {
                let right = Vec3::new(-c.yaw.cos(), 0.0, c.yaw.sin());
                let mut out = c.pos + right * 3.2;
                if blocked(&self.buildings, out.x, out.z, 0.5) { out = c.pos - right * 3.2; }
                pl.pos = out;
                pl.yaw = c.yaw;
                pl.car = None;
                audio::play("enter", 120);
                pl.enter_cd = 0.6;
                self.cam_yaw = c.yaw;
                self.toast = "EXITED VEHICLE".to_string();
                self.toast_time = 2.4;
            }
        } else {
            let fwd = Vec3::new(self.cam_yaw.sin(), 0.0, self.cam_yaw.cos());
            let right = Vec3::new(-self.cam_yaw.cos(), 0.0, self.cam_yaw.sin());
            let f = ((input.down(KeyCode::KeyW) as i32 - input.down(KeyCode::KeyS) as i32) as f32 - pad.move_y).clamp(-1.0, 1.0);
            let r = ((input.down(KeyCode::KeyD) as i32 - input.down(KeyCode::KeyA) as i32) as f32 + pad.move_x).clamp(-1.0, 1.0);
            let mut move_dir = fwd * f + right * r;
            let moving = move_dir.length_squared() > 0.01;
            if moving { move_dir = move_dir.normalize(); }
            let speed = if input.down(KeyCode::ShiftLeft) || input.down(KeyCode::ShiftRight) || pad.sprint { 12.0 } else { 7.0 };

            if input.left || pad.fire { pl.yaw = self.cam_yaw; }
            else if moving { pl.yaw = move_dir.x.atan2(move_dir.z); }

            if moving {
                let nx = pl.pos.x + move_dir.x * speed * dt;
                let nz = pl.pos.z + move_dir.z * speed * dt;
                if !blocked(&self.buildings, nx, nz, 0.45) && nx.abs() < 160.0 && nz.abs() < 160.0 {
                    pl.pos.x = nx; pl.pos.z = nz;
                } else {
                    if !blocked(&self.buildings, nx, pl.pos.z, 0.45) { pl.pos.x = nx; }
                    if !blocked(&self.buildings, pl.pos.x, nz, 0.45) { pl.pos.z = nz; }
                }
                pl.walk += dt * speed * 1.2;
            }

            if (input.pressed(KeyCode::Space) || pad.jump_pressed()) && pl.grounded {
                pl.vy = 9.0;
                pl.grounded = false;
            }
            pl.vy -= 24.0 * dt;
            pl.pos.y += pl.vy * dt;
            if pl.pos.y <= 0.0 { pl.pos.y = 0.0; pl.vy = 0.0; pl.grounded = true; }

            if input.left && input.grabbed || pad.fire {
                let p = self.cam_pitch.min(0.6);
                let dir = Vec3::new(self.cam_yaw.sin() * p.cos(), -p.sin() * 0.45, self.cam_yaw.cos() * p.cos()).normalize();
                let origin = pl.pos + Vec3::new(0.0, 1.35, 0.0) + fwd * 0.6;
                let Game { bullets, particles, shake, rng, toast, toast_time, gun_recoil, rumble_fire, .. } = self;
                fire_player_weapon(pl, bullets, particles, shake, rng, toast, toast_time, gun_recoil, rumble_fire, origin, dir, false);
            }
            let spec = pl.spec();
            if input.pressed(KeyCode::KeyR) && pl.ammo() < spec.mag && pl.reserve() > 0 && pl.reload <= 0.0 {
                pl.reload = spec.reload_time;
            }

            if (input.pressed(KeyCode::KeyE) || pad.enter_pressed()) && pl.grounded && pl.pos.y < 0.2 && pl.enter_cd <= 0.0 {
                let mut best = None;
                let mut bd = 5.5f32;
                for (i, c) in self.cars.iter().enumerate() {
                    let d = (c.pos.x - pl.pos.x).hypot(c.pos.z - pl.pos.z);
                    if d < bd { bd = d; best = Some(i); }
                }
                if let Some(i) = best {
                    let police = self.cars[i].is_police;
                    self.cars[i].speed = 0.0;
                    pl.car = Some(i);
                    audio::play("enter", 120);
                    pl.enter_cd = 0.6;
                    self.cam_yaw = self.cars[i].yaw;
                    pl.stolen += 1;
                    if pl.stolen == 1 || self.rng.chance(0.3) { pl.add_wanted(1.0); }
                    self.toast = if police { "CARJACKED A COP CAR!".to_string() } else { "ENTERED VEHICLE".to_string() };
                    self.toast_time = 2.4;
                }
            }
        }
    }

    fn update_peds(&mut self, dt: f32) {
        let plpos = if let Some(ci) = self.player.car { self.cars[ci].pos } else { self.player.pos };
        let wanted = self.player.wanted;
        for i in 0..self.peds.len() {
            let p = &mut self.peds[i];
            if p.dead {
                p.dead_time += dt;
                continue;
            }
            if p.health <= 0.0 {
                p.dead = true; p.dead_time = 0.0;
                if !p.is_police { self.player.money += 12; }
                continue;
            }
            let d = (p.pos.x - plpos.x).hypot(p.pos.z - plpos.z);
            if p.is_police && wanted > 0 {
                p.target_yaw = (plpos.x - p.pos.x).atan2(plpos.z - p.pos.z);
                p.speed = 5.2;
                p.fire_cd -= dt;
                if d < 26.0 && p.fire_cd <= 0.0 {
                    p.fire_cd = self.rng.f(0.6, 1.3);
                    let dir = (plpos + Vec3::new(0.0, 1.2, 0.0) - (p.pos + Vec3::new(0.0, 1.35, 0.0))).normalize();
                    let origin = p.pos + Vec3::new(0.0, 1.35, 0.0);
                    let Game { bullets, particles, shake, rng, .. } = self;
                    fire_into(bullets, particles, shake, rng, origin, dir, 1, 8.0, 70.0);
                }
            } else if p.flee > 0.0 {
                p.flee -= dt;
                p.speed = 5.4;
                p.target_yaw = (p.pos.x - plpos.x).atan2(p.pos.z - plpos.z);
            } else {
                p.timer -= dt;
                if p.timer <= 0.0 {
                    p.timer = self.rng.f(2.0, 6.0);
                    p.target_yaw = self.rng.f(0.0, 6.28);
                }
                p.speed = self.rng.f(1.2, 2.6);
                if wanted > 0 && d < 12.0 && self.rng.chance(dt * 0.6) { p.flee = 3.0; }
            }
            p.yaw += ang_diff(p.yaw, p.target_yaw) * dt.min(1.0) * 3.0;
            let nx = p.pos.x + p.yaw.sin() * p.speed * dt;
            let nz = p.pos.z + p.yaw.cos() * p.speed * dt;
            if !blocked(&self.buildings, nx, nz, 0.45) && nx.abs() < 158.0 && nz.abs() < 158.0 {
                p.pos.x = nx; p.pos.z = nz;
                p.walk += dt * p.speed * 1.3;
            } else {
                p.target_yaw += PI + self.rng.f(-0.5, 0.5);
            }
        }
    }

    fn update_cars(&mut self, dt: f32) {
        let plpos = if let Some(ci) = self.player.car { self.cars[ci].pos } else { self.player.pos };
        let wanted = self.player.wanted;
        let mut car_push = Vec3::ZERO;
        let mut car_crash = false;
        let mut car_explosions: Vec<Vec3> = Vec::new();
        for i in 0..self.cars.len() {
            if self.player.car == Some(i) { continue; }
            if self.cars[i].health <= 0.0 { continue; }

            let (ahead_x, ahead_z) = {
                let c0 = &self.cars[i];
                (c0.pos.x + c0.yaw.sin() * 4.0, c0.pos.z + c0.yaw.cos() * 4.0)
            };
            let mut car_blocked = false;
            for (j, o) in self.cars.iter().enumerate() {
                if j == i || o.health <= 0.0 { continue; }
                if (o.pos.x - ahead_x).powi(2) + (o.pos.z - ahead_z).powi(2) < 9.0 {
                    car_blocked = true;
                    break;
                }
            }
            let building_blocked = blocked(&self.buildings, ahead_x, ahead_z, 1.5);

            let c = &mut self.cars[i];
            c.siren += dt * 9.0;
            if c.traffic {
                if c.is_police && wanted > 0 {
                    let ty = (plpos.x - c.pos.x).atan2(plpos.z - c.pos.z);
                    c.yaw += ang_diff(c.yaw, ty) * dt.min(1.0) * 2.0;
                    c.target_speed = 17.0;
                    c.speed += (c.target_speed - c.speed) * dt.min(1.0);
                } else {
                    c.speed += (c.target_speed - c.speed) * dt.min(1.0) * 0.8;
                    if building_blocked || car_blocked {
                        c.target_speed = -c.target_speed;
                        c.yaw += dt * 2.4;
                    }
                }
                let nx = c.pos.x + c.yaw.sin() * c.speed * dt;
                let nz = c.pos.z + c.yaw.cos() * c.speed * dt;
                if blocked(&self.buildings, nx, nz, 1.2) || nx.abs() > 170.0 || nz.abs() > 190.0 {
                    c.speed *= -0.4;
                    c.yaw += PI * 0.6;
                } else {
                    c.pos.x = nx; c.pos.z = nz;
                }
            } else {
                c.speed *= (1.0 - dt * 2.0).max(0.0);
            }

            // Car impacts always cost HP, with a short cooldown so it never stun-locks.
            if self.player.car.is_none() && c.speed.abs() > 5.0 {
                let dx = self.player.pos.x - c.pos.x;
                let dz = self.player.pos.z - c.pos.z;
                if dx * dx + dz * dz < 2.0 * 2.0 {
                    let dmg = (28.0 * (c.speed.abs() / 12.0)).min(55.0);
                    if self.player.try_hurt(dmg) {
                        self.rumble_hit = self.rumble_hit.saturating_add(1).min(3);
                        let len = dx.hypot(dz).max(0.01);
                        let kx = self.player.pos.x + dx / len * 1.1;
                        let kz = self.player.pos.z + dz / len * 1.1;
                        if !blocked(&self.buildings, kx, kz, 0.5) {
                            self.player.pos.x = kx;
                            self.player.pos.z = kz;
                        }
                        self.player.vy = 3.2;
                        self.player.grounded = false;
                        self.shake = self.shake.max(0.9);
                        self.toast = "HIT BY CAR".to_string();
                        self.toast_time = 1.0;
                    }
                }
            } else if self.player.car.is_some() && c.speed.abs() > 8.0 {
                let dx = plpos.x - c.pos.x;
                let dz = plpos.z - c.pos.z;
                if dx * dx + dz * dz < 2.6 * 2.6 {
                    let len = dx.hypot(dz).max(0.01);
                    car_push = Vec3::new(dx / len, 0.0, dz / len) * 1.6;
                    car_crash = true;
                    if c.speed.abs() > 16.0 {
                        c.health = 0.0;
                        car_explosions.push(c.pos + Vec3::new(0.0, 1.0, 0.0));
                    }
                    c.speed *= 0.5;
                    if self.player.try_hurt(14.0) {
                        self.rumble_hit = self.rumble_hit.saturating_add(1).min(3);
                        self.shake = self.shake.max(0.7);
                        self.toast = "CAR CRASH".to_string();
                        self.toast_time = 1.0;
                    }
                }
            }
        }
        for p in car_explosions { self.explode_car_at(p); }
        if car_crash {
            if let Some(pci) = self.player.car {
                let pc = &mut self.cars[pci];
                pc.pos += car_push;
                pc.speed *= 0.7;
            }
            self.shake = self.shake.max(0.5);
        }
    }

    fn update_bullets(&mut self, dt: f32) {
        let mut exploded: Vec<Vec3> = Vec::new();
        for i in (0..self.bullets.len()).rev() {
            let b = &mut self.bullets[i];
            b.pos += b.vel * dt;
            b.life -= dt;
            let p = b.pos;
            let mut hit = p.y < 0.05 || b.life <= 0.0 || blocked(&self.buildings, p.x, p.z, 0.05) || p.y > 40.0;
            if !hit && b.owner == 0 {
                for ped in &mut self.peds {
                    if ped.dead { continue; }
                    if (p.x - ped.pos.x).powi(2) + (p.z - ped.pos.z).powi(2) < 0.5 && p.y < 2.2 {
                        ped.health -= b.dmg;
                        self.hit_marker = 0.18;
                        if ped.health <= 0.0 {
                            ped.dead = true; ped.dead_time = 0.0;
                            if !ped.is_police { self.player.money += 12; }
                        }
                        hit = true; break;
                    }
                }
                if !hit {
                    for c in &mut self.cars {
                        if c.health <= 0.0 { continue; }
                        if (p.x - c.pos.x).powi(2) + (p.z - c.pos.z).powi(2) < 2.6 && p.y < 2.2 {
                            c.health -= b.dmg * 0.5;
                            hit = true;
                            if c.health <= 0.0 {
                                c.health = 0.0;
                                exploded.push(c.pos + Vec3::new(0.0, 1.0, 0.0));
                            }
                            break;
                        }
                    }
                }
            } else if !hit {
                let pl = &mut self.player;
                let pp = if let Some(ci) = pl.car { self.cars[ci].pos } else { pl.pos };
                if (p.x - pp.x).powi(2) + (p.z - pp.z).powi(2) < 0.6 && p.y < 2.2 && pl.car.is_none() {
                    pl.hurt(b.dmg);
                    self.rumble_hit = self.rumble_hit.saturating_add(1).min(3);
                    hit = true;
                }
            }
            if hit {
                for _ in 0..4 {
                    self.particles.push(Particle {
                        pos: p,
                        vel: Vec3::new(self.rng.f(-1.0, 1.0), self.rng.f(0.0, 1.0), self.rng.f(-1.0, 1.0)) * 3.0,
                        life: 0.2, max: 0.2, size: 0.1, color: [1.0, 0.7, 0.3, 1.0],
                    });
                }
                self.bullets.remove(i);
            }
        }
        for p in exploded { self.explode_car_at(p); }
    }

    fn update_particles(&mut self, dt: f32) {
        for i in (0..self.particles.len()).rev() {
            let p = &mut self.particles[i];
            p.life -= dt;
            p.vel.y -= 8.0 * dt;
            p.pos += p.vel * dt;
            if p.life <= 0.0 { self.particles.remove(i); }
        }
        if self.particles.len() > 900 { self.particles.drain(0..self.particles.len() - 900); }
    }

    fn update_mission(&mut self, dt: f32) {
        if self.mission.stage == 0 {
            self.mission.timer -= dt;
            if self.mission.timer <= 0.0 {
                let (p, _) = self.road_point();
                self.mission.dest = p;
                self.mission.stage = 1;
                self.mission.reward = self.rng.i(400, 1200);
                self.say("NEW MISSION: THE BIG SCORE");
            }
            return;
        }
        let pp = if let Some(ci) = self.player.car { self.cars[ci].pos } else { self.player.pos };
        let d = (pp.x - self.mission.dest.x).hypot(pp.z - self.mission.dest.y);
        if self.mission.stage == 1 {
            if d < 7.0 {
                let mut marked = 0;
                for i in 0..self.peds.len() {
                    if !self.peds[i].dead && !self.peds[i].is_police && marked < 3 {
                        let dist = (self.peds[i].pos.x - pp.x).hypot(self.peds[i].pos.z - pp.z);
                        if dist < 60.0 {
                            self.peds[i].target = true;
                            marked += 1;
                        }
                    }
                }
                self.mission.targets = self.peds.iter().enumerate().filter(|(_, p)| p.target).map(|(i, _)| i).collect();
                self.mission.stage = 2;
                self.say("ELIMINATE THE TARGETS");
            }
        } else if self.mission.stage == 2 {
            let alive = self.mission.targets.iter().filter(|&&i| !self.peds.get(i).map_or(true, |p| p.dead)).count();
            if alive == 0 {
                self.mission.dest = self.mission.home;
                self.mission.stage = 3;
                for p in &mut self.peds { p.target = false; }
                self.say("RETURN TO THE SAFEHOUSE");
            }
        } else if self.mission.stage == 3 {
            if d < 8.0 {
                self.player.money += self.mission.reward;
                audio::play("mission", 400);
                self.say("MISSION COMPLETE");
                self.mission.stage = 1;
                self.mission.timer = 6.0;
            }
        }
    }

    fn update_camera(&mut self, aspect: f32, _input: &Input, _active: bool) {
        let in_car = self.player.car.is_some();
        let base = if let Some(ci) = self.player.car { self.cars[ci].pos } else { self.player.pos };
        let pitch = self.cam_pitch - self.gun_recoil;
        let cp = pitch.cos();
        let sp = pitch.sin();
        let fwd = Vec3::new(self.cam_yaw.sin() * cp, -sp, self.cam_yaw.cos() * cp);
        let mut pos = base + Vec3::new(0.0, if in_car { 1.3 } else { 1.62 }, 0.0);
        if !in_car && self.player.grounded {
            pos.y += (self.player.walk * 2.0).sin() * 0.045;
        }
        if self.shake > 0.0 {
            let s = self.shake * 0.06;
            pos.x += self.rng.f(-1.0, 1.0) * s;
            pos.y += self.rng.f(-1.0, 1.0) * s;
            pos.z += self.rng.f(-1.0, 1.0) * s;
        }
        let focus = pos + fwd * 10.0;
        let view = Mat4::look_at_rh(pos, focus, Vec3::Y);
        let proj = Mat4::perspective_rh(70.0 * PI / 180.0, aspect, 0.1, 1400.0);
        self.cam_vp = proj * view;
        self.cam_inv = self.cam_vp.inverse();
        self.cam_pos = pos;
    }

    pub fn globals(&self, w: u32, h: u32) -> Globals {
        let t = ((self.day_time - 6.0) / 12.0) * PI;
        let day = (t.sin() * 1.4 + 0.18).clamp(0.05, 1.0);
        let sun = Vec3::new(t.cos() * 0.9, t.sin(), 0.35).normalize();
        Globals {
            view_proj: self.cam_vp.to_cols_array_2d(),
            inv_view_proj: self.cam_inv.to_cols_array_2d(),
            camera_pos: [self.cam_pos.x, self.cam_pos.y, self.cam_pos.z, 0.0],
            sun_dir: [sun.x, sun.y, sun.z, 0.0],
            params: [self.time, day, 0.0025, 0.0],
            screen: [w as f32, h as f32, 0.0, 0.0],
        }
    }

    pub fn collect(&self, out: &mut Vec<Instance>) {
        out.extend_from_slice(&self.statics);

        // blocky clouds drifting overhead
        for i in 0..7u32 {
            let cx = ((i as f32 * 137.0 + self.time * 2.0) % 400.0) - 200.0;
            let cz = (i as f32 * 71.0 % 300.0) - 150.0;
            let cy = 58.0 + (i % 3) as f32 * 6.0;
            out.push(inst(Vec3::new(cx, cy, cz), Vec3::new(16.0, 3.0, 10.0), 0.0, [0.95, 0.96, 1.0, 1.0], -1.0, (1.0, 1.0), 0.0));
            out.push(inst(Vec3::new(cx + 8.0, cy + 1.5, cz + 2.0), Vec3::new(12.0, 3.0, 10.0), 0.0, [0.98, 0.98, 1.0, 1.0], -1.0, (1.0, 1.0), 0.0));
            out.push(inst(Vec3::new(cx - 7.0, cy - 1.0, cz - 3.0), Vec3::new(10.0, 2.5, 8.0), 0.0, [0.92, 0.94, 0.98, 1.0], -1.0, (1.0, 1.0), 0.0));
        }

        if let Some(ci) = self.player.car {
            // First-person cockpit, built relative to the camera so it stays framed.
            let c = &self.cars[ci];
            let cp = self.cam_pitch.cos();
            let sp = self.cam_pitch.sin();
            let fwd = Vec3::new(self.cam_yaw.sin() * cp, -sp, self.cam_yaw.cos() * cp);
            let right = Vec3::new(-self.cam_yaw.cos(), 0.0, self.cam_yaw.sin());
            let up = Vec3::new(0.0, 1.0, 0.0);
            let eye = self.cam_pos;
            let dark = [0.10, 0.11, 0.13, 1.0];

            // dashboard
            out.push(inst(eye + fwd * 1.2 - up * 0.66, Vec3::new(1.7, 0.26, 0.45),
                self.cam_yaw, dark, -1.0, (1.0, 1.0), 0.0));
            // hood / bonnet visible through the windshield
            out.push(inst(eye + fwd * 2.5 - up * 0.72, Vec3::new(1.95, 0.35, 1.7),
                self.cam_yaw, c.color, -1.0, (1.0, 1.0), 0.0));
            // windshield pillars and roof edge
            for s in [-1.0f32, 1.0] {
                out.push(inst(eye + fwd * 1.7 + right * (s * 0.98) + up * 0.35,
                    Vec3::new(0.1, 1.1, 0.12), self.cam_yaw, dark, -1.0, (1.0, 1.0), 0.0));
            }
            out.push(inst(eye + fwd * 1.7 + up * 0.98, Vec3::new(2.1, 0.1, 0.14),
                self.cam_yaw, dark, -1.0, (1.0, 1.0), 0.0));
            // steering wheel (blocky ring)
            let wc = eye + fwd * 0.95 - up * 0.42;
            for (ox, oy, sx, sy) in [
                (0.0, 0.22, 0.44, 0.06),
                (0.0, -0.22, 0.44, 0.06),
                (-0.22, 0.0, 0.06, 0.44),
                (0.22, 0.0, 0.06, 0.44),
            ] {
                out.push(inst(wc + right * ox + up * oy, Vec3::new(sx, sy, 0.09),
                    self.cam_yaw, [0.16, 0.17, 0.19, 1.0], -1.0, (1.0, 1.0), 0.0));
            }
        }
        // No first-person hands or weapon model — the view is clean.
        for p in &self.peds {
            push_human(out, p.pos, p.yaw, p.color, p.walk, p.dead, p.target);
        }
        for (i, c) in self.cars.iter().enumerate() {
            if self.player.car == Some(i) || c.health <= 0.0 { continue; }
            push_car(out, c);
        }
        for b in &self.bullets {
            out.push(inst(b.pos, Vec3::new(0.12, 0.12, 0.65), 0.0, [1.0, 0.95, 0.6, 1.0], -1.0, (1.0, 1.0), 1.0));
        }
        for p in &self.particles {
            let k = p.life / p.max;
            let mut c = p.color;
            c[3] = k;
            out.push(inst(p.pos, Vec3::new(p.size * k, p.size * k, p.size * k), 0.0, c, -1.0, (1.0, 1.0), 1.0));
        }
        if self.mission.stage > 0 {
            out.push(inst(Vec3::new(self.mission.dest.x, 4.0, self.mission.dest.y), Vec3::new(1.6, 8.0, 1.6), 0.0,
                [1.0, 0.8, 0.0, 1.0], -1.0, (1.0, 1.0), 1.0));
            out.push(inst(Vec3::new(self.mission.dest.x, 0.2, self.mission.dest.y), Vec3::new(4.0, 0.2, 4.0), 0.0,
                [1.0, 0.8, 0.0, 1.0], -1.0, (1.0, 1.0), 1.0));
        }
    }

    pub fn hud_pixels(&mut self) -> &[u8] {
        let fb = &mut self.hud;
        fb.clear();
        if !self.started {
            // The home screen image is drawn by the renderer; keep the overlay minimal.
            fb.rect(100, 284, 440, 62, hud::BLACK);
            fb.outline(100, 284, 440, 62, hud::YELLOW);
            fb.text(160, 292, "CLICK TO START", 2, hud::GREEN);
            fb.text(110, 313, "WASD MOVE   E ENTER CAR   LMB FIRE   L LOCK   M NEW MAP", 1, hud::DIM);
            fb.text(130, 330, "F5 SAVE   F9 LOAD   (ENCRYPTED SAVE.DAT)", 1, hud::DIM);
            return &fb.px;
        }

        // top bar
        let money = format!("${}", self.player.money);
        fb.text(12, 10, &money, 2, hud::GREEN);
        let stars = "*".repeat(self.player.wanted as usize);
        fb.text(280, 10, &stars, 2, hud::YELLOW);
        let h = (self.day_time as i32) % 24;
        let m = ((self.day_time.fract() * 60.0) as i32) % 60;
        fb.text(560, 10, &format!("{:02}:{:02}", h, m), 2, hud::WHITE);
        fb.text(390, 10, &format!("MAP {}", self.map_seed % 100000), 1, hud::DIM);
        fb.text(470, 10, &format!("FPS {}", self.fps as i32), 1, hud::DIM);

        // mission
        fb.rect(10, 38, 250, 30, hud::BLACK);
        fb.text(14, 42, "MISSION: THE BIG SCORE", 1, hud::YELLOW);
        if self.mission.stage == 2 { fb.text(14, 55, "ELIMINATE MARKED TARGETS", 1, hud::WHITE); }
        else if self.mission.stage == 3 { fb.text(14, 55, "RETURN TO SAFEHOUSE", 1, hud::WHITE); }
        else { fb.text(14, 55, "DRIVE TO THE WAYPOINT", 1, hud::WHITE); }

        // bars
        fb.text(430, 300, "HP", 1, hud::WHITE);
        fb.bar(450, 298, 90, 9, self.player.health / 100.0, hud::GREEN);
        fb.text(430, 314, "AR", 1, hud::WHITE);
        fb.bar(450, 312, 90, 9, self.player.armor / 100.0, hud::BLUE);
        let spec = self.player.spec();
        let ammo = if self.player.reload > 0.0 {
            "RELOADING".to_string()
        } else {
            format!("{}  {} / {}", spec.name, self.player.ammo(), self.player.reserve())
        };
        fb.text(400, 330, &ammo, 1, if self.player.ammo() < 3 { hud::RED } else { hud::WHITE });
        if let Some(ci) = self.player.car {
            let mph = (self.cars[ci].speed.abs() * 2.6) as i32;
            fb.text(560, 300, &format!("{} MPH", mph), 1, hud::YELLOW);
        }

        // crosshair — expands with recoil, flashes red on a hit
        let (cx, cy) = (HUD_W as i32 / 2, HUD_H as i32 / 2);
        let spread = (self.gun_recoil * 90.0) as i32;
        let cc = if self.hit_marker > 0.0 { hud::RED } else { hud::WHITE };
        let g = 2 + spread;
        fb.rect(cx - 8 - g, cy, 6, 2, cc);
        fb.rect(cx + 2 + g, cy, 6, 2, cc);
        fb.rect(cx, cy - 8 - g, 2, 6, cc);
        fb.rect(cx, cy + 2 + g, 2, 6, cc);
        if self.hit_marker > 0.0 {
            fb.rect(cx - 1, cy - 1, 3, 3, hud::RED);
        }

        // minimap
        let mx = 12; let my = 210; let ms = 110; let scale = 0.55;
        fb.rect(mx, my, ms, ms, [10, 16, 14, 220]);
        fb.outline(mx, my, ms, ms, hud::WHITE);
        let pp = if let Some(ci) = self.player.car { self.cars[ci].pos } else { self.player.pos };
        for &v in &self.roads {
            let vx = mx as f32 + ms as f32 / 2.0 + (v - pp.x) * scale;
            let vy = my as f32 + ms as f32 / 2.0 + (v - pp.z) * scale;
            fb.rect(vx as i32, my, 3, ms, [110, 116, 126, 255]);
            fb.rect(mx, vy as i32, ms, 3, [110, 116, 126, 255]);
        }
        for b in &self.buildings {
            let bx = mx as f32 + ms as f32 / 2.0 + (b.min_x - pp.x) * scale;
            let by = my as f32 + ms as f32 / 2.0 + (b.min_z - pp.z) * scale;
            let bw = (b.max_x - b.min_x) * scale;
            let bh = (b.max_z - b.min_z) * scale;
            if bx > mx as f32 + ms as f32 || by > my as f32 + ms as f32 { continue; }
            fb.rect(bx as i32, by as i32, bw as i32, bh as i32, [40, 48, 56, 255]);
        }
        if self.mission.stage > 0 {
            let dx = mx as f32 + ms as f32 / 2.0 + (self.mission.dest.x - pp.x) * scale;
            let dy = my as f32 + ms as f32 / 2.0 + (self.mission.dest.y - pp.z) * scale;
            fb.circle(dx as i32, dy as i32, 3, hud::YELLOW);
        }
        for c in &self.cars {
            if !c.is_police { continue; }
            let dx = mx as f32 + ms as f32 / 2.0 + (c.pos.x - pp.x) * scale;
            let dy = my as f32 + ms as f32 / 2.0 + (c.pos.z - pp.z) * scale;
            fb.rect(dx as i32, dy as i32, 3, 3, hud::BLUE);
        }
        let px = mx + ms / 2;
        let py = my + ms / 2;
        fb.rect(px - 2, py - 2, 4, 4, hud::WHITE);

        // prompt
        if self.player.car.is_none() {
            let near = self.player.grounded && self.cars.iter().any(|c| (c.pos.x - self.player.pos.x).powi(2) + (c.pos.z - self.player.pos.z).powi(2) < 30.0);
            if near { fb.text(270, 250, "E ENTER VEHICLE", 1, hud::YELLOW); }
        } else {
            fb.text(270, 250, "E EXIT / SPACE HANDBRAKE", 1, hud::YELLOW);
        }

        if self.toast_time > 0.0 {
            fb.rect(190, 70, 260, 18, hud::BLACK);
            fb.text(196, 74, &self.toast, 1, hud::YELLOW);
        }

        // Death fade: slowly black out, show WASTED, then fade back in on respawn.
        let death_alpha = if self.player.dead_timer > 0.0 {
            1.0 - (self.player.dead_timer / 1.5).clamp(0.0, 1.0)
        } else { 0.0 };
        let flash_alpha = (self.respawn_flash / 0.7).clamp(0.0, 1.0);
        let fade = death_alpha.max(flash_alpha);
        if fade > 0.0 {
            fb.rect(0, 0, HUD_W as i32, HUD_H as i32, [0, 0, 0, (fade * 255.0) as u8]);
            if death_alpha > 0.3 {
                let a = (((death_alpha - 0.3) / 0.7) * 255.0) as u8;
                fb.text(232, 150, "WASTED", 5, [255, 60, 80, a]);
            }
        }
        &fb.px
    }
}

fn push_human(out: &mut Vec<Instance>, pos: Vec3, yaw: f32, color: [f32; 4], walk: f32, dead: bool, target: bool) {
    let part = |out: &mut Vec<Instance>, local: Vec3, size: Vec3, c: [f32; 4], emis: f32| {
        let off = Vec3::new(local.x * yaw.cos() + local.z * yaw.sin(), local.y, -local.x * yaw.sin() + local.z * yaw.cos());
        out.push(inst(pos + off, size, yaw, c, -1.0, (1.0, 1.0), emis));
    };
    if dead {
        part(out, Vec3::new(0.0, 0.25, 0.0), Vec3::new(0.7, 0.35, 0.4), color, 0.0);
        return;
    }
    let swing = walk.sin() * 0.25;
    part(out, Vec3::new(0.0, 1.2, 0.0), Vec3::new(0.62, 0.8, 0.36), color, 0.0);
    part(out, Vec3::new(0.0, 1.85, 0.0), Vec3::new(0.4, 0.4, 0.4), [0.85, 0.68, 0.5, 1.0], 0.0);
    part(out, Vec3::new(-0.16, 0.42 + swing, 0.0), Vec3::new(0.18, 0.85, 0.2), [0.18, 0.2, 0.3, 1.0], 0.0);
    part(out, Vec3::new(0.16, 0.42 - swing, 0.0), Vec3::new(0.18, 0.85, 0.2), [0.18, 0.2, 0.3, 1.0], 0.0);
    part(out, Vec3::new(-0.4, 1.2 - swing, 0.0), Vec3::new(0.16, 0.7, 0.16), color, 0.0);
    part(out, Vec3::new(0.4, 1.2 + swing, 0.0), Vec3::new(0.16, 0.7, 0.16), color, 0.0);
    if target {
        part(out, Vec3::new(0.0, 2.5, 0.0), Vec3::new(0.35, 0.5, 0.35), [1.0, 0.2, 0.2, 1.0], 1.0);
    }
}

fn push_car(out: &mut Vec<Instance>, c: &Car) {
    let p = c.pos;
    let y = c.yaw;
    out.push(inst(p + Vec3::new(0.0, 0.7, 0.0), Vec3::new(2.0, 0.6, 4.2), y, c.color, -1.0, (1.0, 1.0), 0.0));
    out.push(inst(p + Vec3::new(0.0, 1.35, -0.15), Vec3::new(1.7, 0.55, 2.0), y, [0.08, 0.14, 0.22, 1.0], -1.0, (1.0, 1.0), 0.0));
    out.push(inst(p + Vec3::new(0.0, 1.68, -0.15), Vec3::new(1.7, 0.12, 1.9), y, c.color, -1.0, (1.0, 1.0), 0.0));
    for sx in [-1.0f32, 1.0] {
        for sz in [-1.35f32, 1.35] {
            let off = Vec3::new(sx * 1.02, 0.42, sz);
            let rot = Vec3::new(off.x * y.cos() + off.z * y.sin(), off.y, -off.x * y.sin() + off.z * y.cos());
            out.push(inst(p + rot, Vec3::new(0.28, 0.8, 0.8), y, [0.06, 0.06, 0.07, 1.0], -1.0, (1.0, 1.0), 0.0));
        }
        let hl = Vec3::new(sx * 0.7, 0.78, 2.14);
        let hoff = Vec3::new(hl.x * y.cos() + hl.z * y.sin(), hl.y, -hl.x * y.sin() + hl.z * y.cos());
        out.push(inst(p + hoff, Vec3::new(0.4, 0.16, 0.08), y, [1.0, 0.95, 0.7, 1.0], -1.0, (1.0, 1.0), 1.0));
        let tl = Vec3::new(sx * 0.7, 0.8, -2.14);
        let toff = Vec3::new(tl.x * y.cos() + tl.z * y.sin(), tl.y, -tl.x * y.sin() + tl.z * y.cos());
        out.push(inst(p + toff, Vec3::new(0.4, 0.14, 0.08), y, [1.0, 0.1, 0.1, 1.0], -1.0, (1.0, 1.0), 1.0));
    }
    if c.is_police {
        let on = c.siren.sin() > 0.0;
        out.push(inst(p + Vec3::new(0.0, 1.85, -0.2), Vec3::new(0.5, 0.12, 0.3), y,
            if on { [1.0, 0.05, 0.05, 1.0] } else { [0.2, 0.0, 0.0, 1.0] }, -1.0, (1.0, 1.0), 1.0));
        out.push(inst(p + Vec3::new(0.0, 1.85, 0.15), Vec3::new(0.5, 0.12, 0.3), y,
            if on { [0.1, 0.0, 0.2, 1.0] } else { [0.1, 0.2, 1.0, 1.0] }, -1.0, (1.0, 1.0), 1.0));
    }
}
