//! Round-trip tests for the magicless decode path.
//!
//! `ZSTD_f_zstd1_magicless` strips the 4-byte zstd magic number
//! (`0x28 0xB5 0x2F 0xFD`) from a frame; OpenZFS uses this format on
//! disk to save four bytes per block (see `module/zstd/zfs_zstd.c`).
//!
//! These tests build a magicless frame by encoding through the
//! standard path and trimming the leading magic, then confirming
//! `FrameDecoder::set_magicless` + `decode_all` reproduces the input.

use structured_zstd::decoding::FrameDecoder;
use structured_zstd::encoding::{CompressionLevel, compress_to_vec};

const MAGIC_LEN: usize = 4;
const MAGIC_BYTES: [u8; MAGIC_LEN] = [0x28, 0xB5, 0x2F, 0xFD];

fn strip_magic(framed: Vec<u8>) -> Vec<u8> {
    assert!(
        framed.len() >= MAGIC_LEN && framed[..MAGIC_LEN] == MAGIC_BYTES,
        "expected a standard zstd frame to strip"
    );
    framed[MAGIC_LEN..].to_vec()
}

fn round_trip(plain: &[u8]) {
    let framed = compress_to_vec(plain.as_ref(), CompressionLevel::Default);
    let magicless = strip_magic(framed);
    let mut decoder = FrameDecoder::new();
    decoder.set_magicless(true);
    let mut out = vec![0u8; plain.len()];
    let written = decoder
        .decode_all(&magicless, &mut out)
        .expect("decode_all (magicless)");
    assert_eq!(written, plain.len(), "wrote correct number of bytes");
    assert_eq!(out, plain, "round trip");
}

#[test]
fn magicless_zeros_round_trip() {
    round_trip(&vec![0u8; 4096]);
}

#[test]
fn magicless_repeating_pattern_round_trip() {
    let mut buf = Vec::with_capacity(4096);
    while buf.len() < 4096 {
        buf.extend_from_slice(b"ABCDEFGH");
    }
    round_trip(&buf);
}

#[test]
fn magicless_incompressible_round_trip() {
    // xorshift-driven pseudo-random bytes — incompressible, so the
    // encoder emits literal-only blocks. Different code path inside
    // the decoder than the back-reference-heavy cases above.
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    let mut buf = Vec::with_capacity(4096);
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        buf.push((state & 0xff) as u8);
    }
    round_trip(&buf);
}

#[test]
fn magicless_decoder_rejects_short_input() {
    // A single byte — not enough to even read the frame header
    // descriptor.
    let mut decoder = FrameDecoder::new();
    decoder.set_magicless(true);
    let mut out = [0u8; 16];
    assert!(decoder.decode_all(&[0x00], &mut out).is_err());
}

#[test]
fn standard_decoder_rejects_magicless_input() {
    // Sanity check: a magicless frame fed to the magic-checking entry
    // point must fail (either with BadMagicNumber or the related
    // SkipFrame path), proving the two modes are not interchangeable.
    let framed = compress_to_vec(&b"hello world"[..], CompressionLevel::Default);
    let magicless = strip_magic(framed);
    let mut decoder = FrameDecoder::new();
    let mut out = [0u8; 64];
    assert!(decoder.decode_all(&magicless, &mut out).is_err());
}
