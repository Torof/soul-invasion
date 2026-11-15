//! The smallest PNG writer that is a PNG: 8-bit RGB, no filter, the
//! pixels in deflate "stored" blocks (no compression). The report is one
//! HTML file and depends on nothing; a map of a few hundred pixels a side
//! weighs a few hundred kilobytes this way, and the bytes are the same
//! for the same picture, which the determinism test relies on.

/// The CRC-32 of `bytes` (IEEE, as PNG chunks carry it).
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let low = crc & 1;
            crc >>= 1;
            if low == 1 {
                crc ^= 0xEDB8_8320;
            }
        }
    }
    !crc
}

/// The Adler-32 of `bytes` (what zlib streams end with).
fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// A zlib stream holding `raw` in stored blocks of up to 65,535 bytes.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(block) = blocks.next() {
        let last = u8::from(blocks.peek().is_none());
        let len = u16::try_from(block.len()).expect("blocks are cut under 65,536");
        out.push(last);
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(block);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(
        &u32::try_from(data.len())
            .expect("a chunk under 4 GB")
            .to_be_bytes(),
    );
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// The PNG of `width` × `height` pixels, `rgb` holding three bytes a pixel,
/// row by row.
pub fn encode(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let row = usize::try_from(width).expect("fits") * 3;
    assert_eq!(
        rgb.len(),
        row * usize::try_from(height).expect("fits"),
        "three bytes a pixel"
    );
    let mut raw = Vec::with_capacity(rgb.len() + usize::try_from(height).expect("fits"));
    for line in rgb.chunks(row.max(1)) {
        raw.push(0); // filter: none
        raw.extend_from_slice(line);
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, RGB, deflate, no filter, no interlace
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, *b"IEND", &[]);
    out
}

/// `bytes` as base64, for a `data:` URL.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let n = group
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= group.len() {
                let index = usize::try_from((n >> (18 - 6 * i)) & 63).expect("six bits");
                out.push(char::from(TABLE[index]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksums_are_the_standard_ones() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn base64_is_the_standard_one() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_png_has_its_signature_header_and_chunks() {
        let png = encode(2, 1, &[255, 0, 0, 0, 0, 255]);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(&png[16..24], &[0, 0, 0, 2, 0, 0, 0, 1]);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        // The IDAT holds a zlib stream of one stored block: the raw row is
        // the filter byte and the six channel bytes.
        // Signature 8, then IHDR 4 + 4 + 13 + 4: the IDAT's data starts at 41.
        let idat = 41;
        assert_eq!(&png[idat - 4..idat], b"IDAT");
        assert_eq!(&png[idat..idat + 2], &[0x78, 0x01]);
        assert_eq!(&png[idat + 2..idat + 7], &[1, 7, 0, 0xF8, 0xFF]);
        assert_eq!(&png[idat + 7..idat + 14], &[0, 255, 0, 0, 0, 0, 255]);
    }

    #[test]
    fn the_same_picture_gives_the_same_bytes() {
        let a = encode(3, 2, &[7; 18]);
        let b = encode(3, 2, &[7; 18]);
        assert_eq!(a, b);
        assert_ne!(a, encode(3, 2, &[8; 18]));
    }
}
