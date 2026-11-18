use std::collections::HashMap;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

static LAST: OnceLock<Mutex<HashMap<&'static str, Instant>>> = OnceLock::new();

/// Fire-and-forget WAV playback via `aplay`, rate-limited per sound.
pub fn play(name: &'static str, min_gap_ms: u64) {
    let map = LAST.get_or_init(|| Mutex::new(HashMap::new()));
    let now = Instant::now();
    {
        let mut m = match map.lock() { Ok(m) => m, Err(_) => return };
        if let Some(t) = m.get(name) {
            if now.duration_since(*t) < Duration::from_millis(min_gap_ms) { return; }
        }
        m.insert(name, now);
    }
    let path = format!("{}/sfx/{}.wav", env!("CARGO_MANIFEST_DIR"), name);
    let _ = Command::new("aplay").arg("-q").arg(&path).spawn();
}
