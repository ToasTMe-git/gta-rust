use aes_gcm::aead::{rand_core::RngCore, Aead, OsRng};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use std::fs;
use std::path::Path;

const SAVE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/save.dat");
// Fixed key compiled into the binary — the file is encrypted, not plain text.
const KEY: [u8; 32] = [
    0x47, 0x54, 0x41, 0x37, 0x2d, 0x52, 0x55, 0x53, 0x54, 0x45, 0x44, 0x2d,
    0x56, 0x43, 0x49, 0x54, 0x59, 0x21, 0x23, 0x25, 0x27, 0x29, 0x2b, 0x2d,
    0x2f, 0x31, 0x33, 0x35, 0x37, 0x39, 0x3b, 0x3d,
];

#[derive(Clone, Copy)]
pub struct SaveData {
    pub version: u32,
    pub seed: u64,
    pub px: f32,
    pub pz: f32,
    pub health: f32,
    pub armor: f32,
    pub money: i32,
    pub wanted: i32,
    pub heat: f32,
    pub day: f32,
    pub wep: u8,
    pub mags: [i32; 3],
    pub reserves: [i32; 3],
    pub mission_stage: u8,
    pub mission_reward: i32,
}

impl SaveData {
    fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(128);
        b.extend_from_slice(&self.version.to_le_bytes());
        b.extend_from_slice(&self.seed.to_le_bytes());
        b.extend_from_slice(&self.px.to_le_bytes());
        b.extend_from_slice(&self.pz.to_le_bytes());
        b.extend_from_slice(&self.health.to_le_bytes());
        b.extend_from_slice(&self.armor.to_le_bytes());
        b.extend_from_slice(&self.money.to_le_bytes());
        b.extend_from_slice(&self.wanted.to_le_bytes());
        b.extend_from_slice(&self.heat.to_le_bytes());
        b.extend_from_slice(&self.day.to_le_bytes());
        b.push(self.wep);
        for m in self.mags { b.extend_from_slice(&m.to_le_bytes()); }
        for r in self.reserves { b.extend_from_slice(&r.to_le_bytes()); }
        b.push(self.mission_stage);
        b.extend_from_slice(&self.mission_reward.to_le_bytes());
        b
    }

    fn from_bytes(b: &[u8]) -> Option<Self> {
        if b.len() < 4 + 8 + 8 * 4 + 4 * 2 + 1 + 12 + 1 + 4 { return None; }
        let mut i = 0usize;
        macro_rules! take {
            ($t:ty) => {{
                let v = <$t>::from_le_bytes(b[i..i + std::mem::size_of::<$t>()].try_into().ok()?);
                i += std::mem::size_of::<$t>();
                v
            }};
        }
        let version = take!(u32);
        let seed = take!(u64);
        let px = take!(f32);
        let pz = take!(f32);
        let health = take!(f32);
        let armor = take!(f32);
        let money = take!(i32);
        let wanted = take!(i32);
        let heat = take!(f32);
        let day = take!(f32);
        let wep = *b.get(i)?;
        i += 1;
        let mut mags = [0i32; 3];
        for m in &mut mags { *m = take!(i32); }
        let mut reserves = [0i32; 3];
        for r in &mut reserves { *r = take!(i32); }
        let mission_stage = *b.get(i)?;
        i += 1;
        let mission_reward = take!(i32);
        if version != 1 { return None; }
        Some(Self {
            version, seed, px, pz, health, armor, money, wanted, heat, day,
            wep, mags, reserves, mission_stage, mission_reward,
        })
    }
}

pub fn exists() -> bool { Path::new(SAVE_PATH).exists() }

pub fn save(data: &SaveData) -> Result<(), String> {
    let cipher = Aes256Gcm::new_from_slice(&KEY).map_err(|_| "bad key".to_string())?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher_text = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), data.to_bytes().as_slice())
        .map_err(|_| "encryption failed".to_string())?;
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&cipher_text);
    fs::write(SAVE_PATH, out).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_roundtrip() {
        let d = SaveData {
            version: 1, seed: 123456789, px: 12.5, pz: -30.0,
            health: 75.0, armor: 20.0, money: 999, wanted: 3, heat: 0.5,
            day: 18.25, wep: 2, mags: [12, 6, 30], reserves: [120, 48, 180],
            mission_stage: 2, mission_reward: 750,
        };
        save(&d).unwrap();
        let raw = fs::read(SAVE_PATH).unwrap();
        assert!(!raw.windows(7).any(|w| w == b"123456789"));
        let e = load().unwrap();
        assert_eq!(e.seed, d.seed);
        assert_eq!(e.money, d.money);
        assert_eq!(e.mags, d.mags);
        assert_eq!(e.mission_reward, d.mission_reward);
    }
}

pub fn load() -> Result<SaveData, String> {
    let raw = fs::read(SAVE_PATH).map_err(|e| e.to_string())?;
    if raw.len() < 28 { return Err("save file too short".to_string()); }
    let (nonce, body) = raw.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(&KEY).map_err(|_| "bad key".to_string())?;
    let plain = cipher
        .decrypt(Nonce::from_slice(nonce), body)
        .map_err(|_| "decryption failed / corrupted save".to_string())?;
    SaveData::from_bytes(&plain).ok_or_else(|| "invalid save data".to_string())
}
