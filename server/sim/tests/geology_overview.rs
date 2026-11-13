//! An overview of the rock regions of a 6 km world (D42), seen from
//! above, for the project lead to judge them at a glance. Run by hand:
//!
//! ```text
//! SOUL_OVERVIEW_OUT=path/to/geologie-6km.png \
//!     cargo test -p sim --test geology_overview -- --ignored
//! ```
//!
//! One pixel every 6 m, a grey line every kilometre. Granite pinkish
//! grey, limestone cream, chalk white, sandstone ochre (the client's
//! tints, at full strength). Written with no dependency: a palette PNG
//! whose image data is stored, not compressed.

use sim::{Geology, Rock};

/// Side of the world, in metres (tiles).
const WORLD_M: i32 = 6144;
/// Metres a pixel.
const PIXEL_M: i32 = 6;

#[test]
#[ignore = "writes a picture for the project lead: run by hand"]
fn write_the_overview_of_a_six_km_world() {
    let geology: Geology =
        ron::from_str(include_str!("../../../content/geology.ron")).expect("geology.ron parses");
    let side = WORLD_M / PIXEL_M;
    let mut pixels = Vec::new();
    for py in 0..side {
        for px in 0..side {
            let (x, y) = (px * PIXEL_M, py * PIXEL_M);
            let km = |v: i32| v % 1000 < PIXEL_M;
            pixels.push(if km(x) || km(y) {
                4
            } else {
                match geology.rock_at(sim::VALLEY_SEED, x, y) {
                    Rock::Granite => 0,
                    Rock::Limestone => 1,
                    Rock::Chalk => 2,
                    Rock::Sandstone => 3,
                }
            });
        }
    }
    let palette = [
        [0xb8, 0x99, 0x9c],
        [0xef, 0xe3, 0xbf],
        [0xff, 0xff, 0xff],
        [0xc9, 0x8a, 0x3c],
        [0x80, 0x80, 0x80],
    ];
    let side = u32::try_from(side).expect("a small picture");
    let png = palette_png(side, side, &palette, &pixels);
    let out = std::env::var("SOUL_OVERVIEW_OUT").unwrap_or_else(|_| "geologie-6km.png".into());
    std::fs::write(&out, png).expect("the picture written");
    println!("wrote {out}");
}

/// A PNG of `width × height` palette indices, row by row.
fn palette_png(width: u32, height: u32, palette: &[[u8; 3]], pixels: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(pixels.len() + height as usize);
    for row in pixels.chunks(width as usize) {
        raw.push(0); // no filter
        raw.extend_from_slice(row);
    }
    // A zlib stream of stored deflate blocks.
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (k, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(k + 1 == blocks.len()));
        let len = u16::try_from(block.len()).expect("a stored block");
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&(b << 16 | a).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 3, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    chunk(&mut png, *b"IHDR", &header);
    chunk(&mut png, *b"PLTE", palette.concat().as_slice());
    chunk(&mut png, *b"IDAT", &zlib);
    chunk(&mut png, *b"IEND", &[]);
    png
}

/// Appends a PNG chunk: its length, kind, data and CRC.
fn chunk(png: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    png.extend_from_slice(&u32::try_from(data.len()).expect("a chunk").to_be_bytes());
    let start = png.len();
    png.extend_from_slice(&kind);
    png.extend_from_slice(data);
    let mut crc = !0_u32;
    for byte in &png[start..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
}
