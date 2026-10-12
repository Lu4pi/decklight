//! Minimaler PNG-Encoder für erzeugte Cover (unkomprimiert, ohne Abhängigkeiten).

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// RGB-Bild (`w`×`h`, Zeilen hintereinander) als PNG.
pub fn png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w as usize * 3 + 1) * h as usize);
    for row in rgb.chunks_exact(w as usize * 3).take(h as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    // zlib mit "stored"-Blöcken (max. 65535 Bytes je Block).
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Cover mit Farbverlauf und Kreisen, abgeleitet aus der Track-ID.
pub fn cover(seed: u32) -> Vec<u8> {
    let size = 160u32;
    let hue = |s: u32| -> [f64; 3] {
        let h = f64::from(s % 360) / 60.0;
        let x = 1.0 - (h % 2.0 - 1.0).abs();
        match h as u32 {
            0 => [1.0, x, 0.0],
            1 => [x, 1.0, 0.0],
            2 => [0.0, 1.0, x],
            3 => [0.0, x, 1.0],
            4 => [x, 0.0, 1.0],
            _ => [1.0, 0.0, x],
        }
    };
    let a = hue(seed.wrapping_mul(97));
    let b = hue(seed.wrapping_mul(97) + 140);
    let mut rgb = Vec::with_capacity((size * size * 3) as usize);
    for y in 0..size {
        for x in 0..size {
            let t = f64::from(x + y) / f64::from(2 * size);
            let (dx, dy) = (f64::from(x) - 80.0, f64::from(y) - 80.0);
            let ring = ((dx * dx + dy * dy).sqrt() / 12.0).sin().abs();
            for c in 0..3 {
                let v = (a[c] * (1.0 - t) + b[c] * t) * (0.55 + 0.45 * ring);
                rgb.push((v * 230.0) as u8);
            }
        }
    }
    png(size, size, &rgb)
}
