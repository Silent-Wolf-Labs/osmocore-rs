//! Results the C recording can't hold: inputs where C reads or writes out of bounds or
//! overflows, and the guarantees the preferred API adds.

use osmocore_rs::compat::gea as compat;
use osmocore_rs::gsm::gea::*;

const KC: [u8; GEA3_KEY_BYTES] = [0x2b, 0xd6, 0x45, 0x9f, 0x82, 0xc5, 0xbc, 0x00];

#[test]
fn compat_rejects_buffers_c_would_overrun() {
    let mut out = [7; 8];
    // The output is shorter than the length.
    assert_eq!(compat::gprs_cipher_run(&mut out, 9, 3, &KC, 0, 0), -22);
    // GEA3 reads 8 key bytes and GEA4 16.
    assert_eq!(compat::gprs_cipher_run(&mut out, 8, 3, &KC[..7], 0, 0), -22);
    assert_eq!(compat::gprs_cipher_run(&mut out, 8, 4, &KC, 0, 0), -22);
    assert_eq!(out, [7; 8], "an error leaves the output alone");
}

#[test]
fn compat_reads_only_the_key_bytes_it_needs() {
    let (mut short, mut long) = ([0; 8], [0; 8]);
    let mut padded = [0xff; 16];
    padded[..8].copy_from_slice(&KC);
    assert_eq!(compat::gprs_cipher_run(&mut short, 8, 3, &KC, 1, 1), 0);
    assert_eq!(compat::gprs_cipher_run(&mut long, 8, 3, &padded, 1, 1), 0);
    assert_eq!(short, long);
}

#[test]
fn compat_input_ui_wraps_where_c_overflows() {
    // C's SAPI arithmetic overflows an int from SAPI 16 on.
    assert_eq!(compat::gprs_cipher_gen_input_ui(0, 16, 0, 0), 0);
    assert_eq!(compat::gprs_cipher_gen_input_ui(0, 17, 0, 0), 1 << 27);
    assert_eq!(input_ui(0, 16, 0, 0), None);
}

#[test]
fn gea3_is_gea4_with_the_key_repeated() {
    let mut key = [0; GEA4_KEY_BYTES];
    key[..8].copy_from_slice(&KC);
    key[8..].copy_from_slice(&KC);
    let (mut three, mut four) = ([0; 64], [0; 64]);
    gea3(&KC, 0x1234, Direction::Downlink, &mut three);
    gea4(&key, 0x1234, Direction::Downlink, &mut four);
    assert_eq!(three, four);
}

#[test]
fn keystreams_can_be_longer_than_c_allows() {
    let mut long = vec![0; MAX_FRAME_BYTES + 100];
    gea3(&KC, 7, Direction::Uplink, &mut long);
    let mut c = vec![0; MAX_FRAME_BYTES];
    assert_eq!(
        compat::gprs_cipher_run(&mut c, MAX_FRAME_BYTES as u16, 3, &KC, 7, 0),
        0
    );
    assert_eq!(c[..], long[..MAX_FRAME_BYTES]);
}

#[test]
fn a_keystream_ciphers_and_deciphers_a_frame() {
    let frame: Vec<u8> = (0..200).map(|byte| byte as u8).collect();
    let mut keystream = vec![0; frame.len()];
    gea4(
        &[0x5a; GEA4_KEY_BYTES],
        99,
        Direction::Downlink,
        &mut keystream,
    );
    let cipher =
        |data: &[u8]| -> Vec<u8> { data.iter().zip(&keystream).map(|(a, b)| a ^ b).collect() };
    assert_ne!(cipher(&frame), frame);
    assert_eq!(cipher(&cipher(&frame)), frame);
}
