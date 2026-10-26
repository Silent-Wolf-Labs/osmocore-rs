use osmocore_rs::compat::crc::{OSMO_CRC16_CCITT_TABLE, OSMO_CRC16_TABLE};
use osmocore_rs::compat::crc::{
    osmo_crc8gen_check_bits, osmo_crc8gen_compute_bits, osmo_crc8gen_set_bits, osmo_crc16,
    osmo_crc16_byte, osmo_crc16_ccitt, osmo_crc16_ccitt_byte, osmo_crc16gen_check_bits,
    osmo_crc16gen_compute_bits, osmo_crc16gen_set_bits, osmo_crc32gen_check_bits,
    osmo_crc32gen_compute_bits, osmo_crc32gen_set_bits, osmo_crc64gen_check_bits,
    osmo_crc64gen_compute_bits, osmo_crc64gen_set_bits,
};
#[path = "../support/crc_fixture.rs"]
mod fixture;
use fixture::{bytes, number, signed};
use osmocore_rs::crc::*;
const BYTE: &str = include_str!("../compatibility/vectors/crc_byte_vectors.json");
const GENERIC: &str = include_str!("../compatibility/vectors/crc_generic_vectors.json");

#[test]
fn byte_crc_matches_direct_c_observations() {
    for r in fixture::validate_byte(BYTE) {
        let ccitt = number(&r, "flavor") == 1;
        let result = match r["kind"].as_str() {
            "table" => {
                if ccitt {
                    OSMO_CRC16_CCITT_TABLE[number(&r, "byte") as usize]
                } else {
                    OSMO_CRC16_TABLE[number(&r, "byte") as usize]
                }
            }
            "byte" => {
                if ccitt {
                    osmo_crc16_ccitt_byte(number(&r, "init") as u16, number(&r, "byte") as u8)
                } else {
                    osmo_crc16_byte(number(&r, "init") as u16, number(&r, "byte") as u8)
                }
            }
            "buffer" => {
                let input = bytes(&r["before"]);
                let len = number(&r, "len") as usize;
                let split = number(&r, "split") as usize;
                let compute = if ccitt { osmo_crc16_ccitt } else { osmo_crc16 };
                let preferred = if ccitt { crc16_ccitt } else { crc16 };
                let init = number(&r, "init") as u16;
                let mid = compute(init, &input, split).unwrap();
                assert_eq!(u64::from(mid), number(&r, "mid"));
                assert_eq!(
                    u64::from(preferred(init, &input[..split])),
                    number(&r, "mid")
                );
                let continuation = compute(mid, &input[split..], len - split).unwrap();
                assert_eq!(u64::from(continuation), number(&r, "continuation"));
                assert_eq!(
                    u64::from(preferred(mid, &input[split..len])),
                    number(&r, "continuation")
                );
                assert_eq!(
                    u64::from(preferred(init, &input[..len])),
                    number(&r, "result")
                );
                let result = compute(init, &input, len).unwrap();
                assert_eq!(input, bytes(&r["after"]));
                result
            }
            _ => panic!("kind"),
        };
        assert_eq!(
            u64::from(result),
            number(&r, "result"),
            "case {}",
            r["case"]
        );
    }
}

#[test]
fn generic_crc_matches_direct_c_observations() {
    for r in fixture::validate_generic(GENERIC) {
        let input = bytes(&r["input_before"]);
        let len = signed(&r, "len");
        let source = if len <= 0 { &[][..] } else { input.as_slice() };
        let consumed = &source[..len.max(0) as usize];
        let bits = number(&r, "bits") as usize;
        let mut out = bytes(&r["before"]);
        macro_rules! compare {
            ($code:ident, $ty:ty, $compute:ident, $check:ident, $set:ident) => {{
                let code = $code {
                    bits: bits as i32,
                    poly: number(&r, "poly") as $ty,
                    init: number(&r, "init") as $ty,
                    remainder: number(&r, "remainder") as $ty,
                };
                assert_eq!(
                    $compute(&code, source, len).unwrap() as u64,
                    number(&r, "result"),
                    "case {}",
                    r["case"]
                );
                assert_eq!(
                    code.compute_bits(consumed).unwrap() as u64,
                    number(&r, "result")
                );
                let mut preferred_out = out.clone();
                code.set_bits(consumed, &mut preferred_out).unwrap();
                assert_eq!(preferred_out, bytes(&r["after"]));
                $set(&code, source, len, &mut out).unwrap();
                assert_eq!(out, bytes(&r["after"]));
                assert_eq!(
                    $check(&code, source, len, &out).unwrap() as u64,
                    number(&r, "match")
                );
                assert_eq!(
                    code.check_bits(consumed, &out),
                    Ok(number(&r, "match") == 0)
                );
                let flips = bytes(&r["flips"]);
                let nonbinary = bytes(&r["nonbinary"]);
                for bit in 0..bits {
                    out[bit] ^= 1;
                    assert_eq!(
                        $check(&code, source, len, &out).unwrap(),
                        i32::from(flips[bit])
                    );
                    assert_eq!(code.check_bits(consumed, &out), Ok(flips[bit] == 0));
                    out[bit] ^= 1;
                    out[bit] ^= 2;
                    assert_eq!(
                        $check(&code, source, len, &out).unwrap(),
                        i32::from(nonbinary[bit])
                    );
                    assert_eq!(code.check_bits(consumed, &out), Ok(nonbinary[bit] == 0));
                    out[bit] ^= 2;
                }
                assert_eq!(out, bytes(&r["after"]));
            }};
        }
        match number(&r, "width") {
            8 => compare!(
                Crc8Code,
                u8,
                osmo_crc8gen_compute_bits,
                osmo_crc8gen_check_bits,
                osmo_crc8gen_set_bits
            ),
            16 => compare!(
                Crc16Code,
                u16,
                osmo_crc16gen_compute_bits,
                osmo_crc16gen_check_bits,
                osmo_crc16gen_set_bits
            ),
            32 => compare!(
                Crc32Code,
                u32,
                osmo_crc32gen_compute_bits,
                osmo_crc32gen_check_bits,
                osmo_crc32gen_set_bits
            ),
            64 => compare!(
                Crc64Code,
                u64,
                osmo_crc64gen_compute_bits,
                osmo_crc64gen_check_bits,
                osmo_crc64gen_set_bits
            ),
            _ => panic!("width"),
        }
        assert_eq!(input, bytes(&r["input_after"]));
    }
}
#[test]
fn corrupt_crc_evidence_is_rejected() {
    fixture::check_corruptions(BYTE, false);
    fixture::check_corruptions(GENERIC, true);
}
