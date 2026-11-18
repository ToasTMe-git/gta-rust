pub type Color = [u8; 4];

pub const WHITE: Color = [255, 255, 255, 255];
pub const YELLOW: Color = [255, 204, 0, 255];
pub const RED: Color = [255, 60, 90, 255];
pub const GREEN: Color = [61, 220, 132, 255];
pub const BLUE: Color = [74, 168, 255, 255];
pub const DIM: Color = [200, 210, 225, 190];
pub const BLACK: Color = [8, 10, 16, 220];

pub struct Fb {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Fb {
    pub fn new(w: u32, h: u32) -> Self {
        Self { w, h, px: vec![0; (w * h * 4) as usize] }
    }

    pub fn clear(&mut self) {
        self.px.fill(0);
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        if w <= 0 || h <= 0 { return; }
        for yy in y.max(0)..(y + h).min(self.h as i32) {
            for xx in x.max(0)..(x + w).min(self.w as i32) {
                let i = ((yy as u32 * self.w + xx as u32) * 4) as usize;
                let a = c[3] as f32 / 255.0;
                if a >= 1.0 {
                    self.px[i] = c[0];
                    self.px[i + 1] = c[1];
                    self.px[i + 2] = c[2];
                    self.px[i + 3] = 255;
                } else {
                    let da = self.px[i + 3] as f32 / 255.0;
                    let oa = a + da * (1.0 - a);
                    if oa <= 0.0 { continue; }
                    for k in 0..3 {
                        let v = (c[k] as f32 * a + self.px[i + k] as f32 * da * (1.0 - a)) / oa;
                        self.px[i + k] = v as u8;
                    }
                    self.px[i + 3] = (oa * 255.0) as u8;
                }
            }
        }
    }

    pub fn circle(&mut self, cx: i32, cy: i32, r: i32, c: Color) {
        for yy in -r..=r {
            let span = ((r * r - yy * yy) as f32).sqrt() as i32;
            self.rect(cx - span, cy + yy, span * 2 + 1, 1, c);
        }
    }

    pub fn outline(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        self.rect(x, y, w, 1, c);
        self.rect(x, y + h - 1, w, 1, c);
        self.rect(x, y, 1, h, c);
        self.rect(x + w - 1, y, 1, h, c);
    }

    pub fn bar(&mut self, x: i32, y: i32, w: i32, h: i32, pct: f32, c: Color) {
        self.rect(x, y, w, h, [0, 0, 0, 170]);
        self.rect(x + 1, y + 1, ((w - 2) as f32 * pct.clamp(0.0, 1.0)) as i32, h - 2, c);
        self.outline(x, y, w, h, [255, 255, 255, 180]);
    }

    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: Color) {
        let mut cursor = x;
        for ch in s.bytes() {
            let ch = if ch.is_ascii_lowercase() { ch - 32 } else { ch };
            let g = glyph(ch);
            for col in 0..5u32 {
                let bits = g[col as usize];
                for row in 0..7u32 {
                    if bits & (1 << row) != 0 {
                        self.rect(cursor + col as i32 * scale, y + row as i32 * scale, scale, scale, c);
                    }
                }
            }
            cursor += 6 * scale;
        }
    }
}

fn glyph(c: u8) -> [u8; 5] {
    match c {
        b'A' => [0x7C, 0x12, 0x11, 0x12, 0x7C],
        b'B' => [0x7F, 0x49, 0x49, 0x49, 0x36],
        b'C' => [0x3E, 0x41, 0x41, 0x41, 0x22],
        b'D' => [0x7F, 0x41, 0x41, 0x22, 0x1C],
        b'E' => [0x7F, 0x49, 0x49, 0x49, 0x41],
        b'F' => [0x7F, 0x49, 0x49, 0x09, 0x01],
        b'G' => [0x3E, 0x41, 0x49, 0x49, 0x7A],
        b'H' => [0x7F, 0x08, 0x08, 0x08, 0x7F],
        b'I' => [0x00, 0x41, 0x7F, 0x41, 0x00],
        b'J' => [0x20, 0x40, 0x41, 0x3F, 0x01],
        b'K' => [0x7F, 0x08, 0x14, 0x22, 0x41],
        b'L' => [0x7F, 0x40, 0x40, 0x40, 0x40],
        b'M' => [0x7F, 0x02, 0x0C, 0x02, 0x7F],
        b'N' => [0x7F, 0x04, 0x08, 0x10, 0x7F],
        b'O' => [0x3E, 0x41, 0x41, 0x41, 0x3E],
        b'P' => [0x7F, 0x09, 0x09, 0x09, 0x06],
        b'Q' => [0x3E, 0x41, 0x51, 0x21, 0x5E],
        b'R' => [0x7F, 0x09, 0x19, 0x29, 0x46],
        b'S' => [0x46, 0x49, 0x49, 0x49, 0x31],
        b'T' => [0x01, 0x01, 0x7F, 0x01, 0x01],
        b'U' => [0x3F, 0x40, 0x40, 0x40, 0x3F],
        b'V' => [0x1F, 0x20, 0x40, 0x20, 0x1F],
        b'W' => [0x7F, 0x20, 0x18, 0x20, 0x7F],
        b'X' => [0x63, 0x14, 0x08, 0x14, 0x63],
        b'Y' => [0x03, 0x04, 0x78, 0x04, 0x03],
        b'Z' => [0x61, 0x51, 0x49, 0x45, 0x43],
        b'0' => [0x3E, 0x51, 0x49, 0x45, 0x3E],
        b'1' => [0x00, 0x42, 0x7F, 0x40, 0x00],
        b'2' => [0x42, 0x61, 0x51, 0x49, 0x46],
        b'3' => [0x21, 0x41, 0x45, 0x4B, 0x31],
        b'4' => [0x18, 0x14, 0x12, 0x7F, 0x10],
        b'5' => [0x27, 0x45, 0x45, 0x45, 0x39],
        b'6' => [0x3C, 0x4A, 0x49, 0x49, 0x30],
        b'7' => [0x01, 0x71, 0x09, 0x05, 0x03],
        b'8' => [0x36, 0x49, 0x49, 0x49, 0x36],
        b'9' => [0x06, 0x49, 0x49, 0x29, 0x1E],
        b'$' => [0x24, 0x2A, 0x7F, 0x2A, 0x12],
        b':' => [0x00, 0x36, 0x36, 0x00, 0x00],
        b'/' => [0x20, 0x10, 0x08, 0x04, 0x02],
        b'*' => [0x14, 0x08, 0x3E, 0x08, 0x14],
        b'-' => [0x08, 0x08, 0x08, 0x08, 0x08],
        b'.' => [0x00, 0x60, 0x60, 0x00, 0x00],
        b'!' => [0x00, 0x7D, 0x00, 0x00, 0x00],
        b'?' => [0x02, 0x01, 0x51, 0x09, 0x06],
        b'+' => [0x08, 0x08, 0x3E, 0x08, 0x08],
        b'<' => [0x08, 0x14, 0x22, 0x41, 0x00],
        b'>' => [0x00, 0x41, 0x22, 0x14, 0x08],
        _ => [0; 5],
    }
}
