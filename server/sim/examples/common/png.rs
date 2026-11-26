//! An RGB(A) PNG encoder for the examples (`atlas`, `windows_sheet`, `planet`): a
//! small deflate of one fixed-Huffman block, so the drawings add no
//! dependency. Not part of the game.

/// A pixel.
pub type Rgb = [u8; 3];

/// An RGB PNG, rows filtered by whichever of PNG's five filters makes the
/// smallest sum, compressed by [`deflate`].
pub fn png(width: usize, height: usize, pixels: &[Rgb]) -> Vec<u8> {
    let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
    encode(width, height, &flat, 3, 2)
}

/// A pixel with its opacity.
#[allow(dead_code, reason = "only some examples draw transparent layers")]
pub type Rgba = [u8; 4];

/// An RGBA PNG (a layer: transparent where alpha is 0), as [`png`].
#[allow(dead_code, reason = "only some examples draw transparent layers")]
pub fn png_rgba(width: usize, height: usize, pixels: &[Rgba]) -> Vec<u8> {
    let flat: Vec<u8> = pixels.iter().flatten().copied().collect();
    encode(width, height, &flat, 4, 6)
}

/// Bytes of `bpp` per pixel, of PNG colour type `colour`.
fn encode(width: usize, height: usize, flat: &[u8], bpp: usize, colour: u8) -> Vec<u8> {
    let stride = width * bpp;
    let mut raw = Vec::with_capacity((stride + 1) * height);
    let zero = vec![0_u8; stride];
    for y in 0..height {
        let row = &flat[y * stride..(y + 1) * stride];
        let up = if y == 0 {
            &zero[..]
        } else {
            &flat[(y - 1) * stride..y * stride]
        };
        let mut best: Option<(u64, u8, Vec<u8>)> = None;
        for filter in 0..5_u8 {
            let line: Vec<u8> = (0..stride)
                .map(|k| {
                    let a = if k >= bpp { row[k - bpp] } else { 0 };
                    let b = up[k];
                    let c = if k >= bpp { up[k - bpp] } else { 0 };
                    let p = match filter {
                        0 => 0,
                        1 => a,
                        2 => b,
                        3 => u8::midpoint(a, b),
                        _ => paeth(a, b, c),
                    };
                    row[k].wrapping_sub(p)
                })
                .collect();
            let cost: u64 = line
                .iter()
                .map(|&v| u64::from(v.min(0_u8.wrapping_sub(v))))
                .sum();
            if best.as_ref().is_none_or(|b| cost < b.0) {
                best = Some((cost, filter, line));
            }
        }
        let (_, filter, line) = best.expect("a filter");
        raw.push(filter);
        raw.extend_from_slice(&line);
    }
    let mut zlib = vec![0x78, 0x01];
    zlib.extend_from_slice(&deflate(&raw));
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&(b << 16 | a).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&u32::try_from(width).expect("small").to_be_bytes());
    header.extend_from_slice(&u32::try_from(height).expect("small").to_be_bytes());
    header.extend_from_slice(&[8, colour, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &zlib);
    chunk(&mut out, *b"IEND", &[]);
    out
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).expect("a chunk").to_be_bytes());
    let mut crc = !0_u32;
    for byte in kind.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&(!crc).to_be_bytes());
}

/// Bits written least significant first, as deflate wants.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        self.acc |= u64::from(value) << self.n;
        self.n += count;
        while self.n >= 8 {
            self.out
                .push(u8::try_from(self.acc & 0xff).expect("a byte"));
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code, written most significant bit first.
    fn code(&mut self, code: u32, len: u32) {
        let reversed = code.reverse_bits() >> (32 - len);
        self.put(reversed, len);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out
                .push(u8::try_from(self.acc & 0xff).expect("a byte"));
        }
        self.out
    }
}

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// A literal or length symbol in deflate's fixed Huffman code.
fn fixed(bits: &mut Bits, symbol: u32) {
    match symbol {
        0..=143 => bits.code(0x30 + symbol, 8),
        144..=255 => bits.code(0x190 + symbol - 144, 9),
        256..=279 => bits.code(symbol - 256, 7),
        _ => bits.code(0xc0 + symbol - 280, 8),
    }
}

/// Raw deflate: one block of the fixed Huffman code, with LZ77 matches
/// found through a hash of three bytes and a chain of earlier places.
fn deflate(data: &[u8]) -> Vec<u8> {
    const WINDOW: usize = 32_768;
    const CHAIN: usize = 24;
    let mut bits = Bits {
        out: Vec::with_capacity(data.len() / 3),
        acc: 0,
        n: 0,
    };
    bits.put(1, 1); // the last block
    bits.put(1, 2); // fixed Huffman
    let mut head = vec![usize::MAX; 1 << 15];
    let mut prev = vec![usize::MAX; WINDOW];
    let hash3 = |i: usize| -> usize {
        ((usize::from(data[i]) << 10) ^ (usize::from(data[i + 1]) << 5) ^ usize::from(data[i + 2]))
            & 0x7fff
    };
    let mut i = 0;
    let insert = |i: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if i + 2 < data.len() {
            let h = hash3(i);
            prev[i % WINDOW] = head[h];
            head[h] = i;
        }
    };
    while i < data.len() {
        let (mut best_len, mut best_dist) = (0, 0);
        if i + 2 < data.len() {
            let mut candidate = head[hash3(i)];
            let mut tries = 0;
            while candidate != usize::MAX && i - candidate <= WINDOW && tries < CHAIN {
                let max = (data.len() - i).min(258);
                let mut len = 0;
                while len < max && data[candidate + len] == data[i + len] {
                    len += 1;
                }
                if len > best_len {
                    best_len = len;
                    best_dist = i - candidate;
                    if len == max {
                        break;
                    }
                }
                let next = prev[candidate % WINDOW];
                if next == usize::MAX || next >= candidate {
                    break;
                }
                candidate = next;
                tries += 1;
            }
        }
        if best_len >= 3 {
            let len = u16::try_from(best_len).expect("at most 258");
            let k = LENGTH_BASE
                .iter()
                .rposition(|&b| b <= len)
                .expect("a length code");
            fixed(&mut bits, 257 + u32::try_from(k).expect("small"));
            bits.put(u32::from(len - LENGTH_BASE[k]), u32::from(LENGTH_EXTRA[k]));
            let dist = u16::try_from(best_dist).expect("within the window");
            let d = DIST_BASE
                .iter()
                .rposition(|&b| b <= dist)
                .expect("a distance code");
            bits.code(u32::try_from(d).expect("small"), 5);
            bits.put(u32::from(dist - DIST_BASE[d]), u32::from(DIST_EXTRA[d]));
            for k in 0..best_len {
                insert(i + k, &mut head, &mut prev);
            }
            i += best_len;
        } else {
            fixed(&mut bits, u32::from(data[i]));
            insert(i, &mut head, &mut prev);
            i += 1;
        }
    }
    fixed(&mut bits, 256);
    bits.finish()
}
