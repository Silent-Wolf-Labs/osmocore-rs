use osmocore_rs::base64;
use osmocore_rs::bitvec::{BitValue, BitVec, BitVecError};
use osmocore_rs::crc::{crc16, crc16_ccitt};

#[test]
fn deterministic_bit_writes_reads_and_partial_failures_match_a_boolean_model() {
    let padding = [false, false, true, false, true, false, true, true];
    let values = [BitValue::Zero, BitValue::One, BitValue::L, BitValue::H];
    for capacity in [0, 1, 2, 3, 8] {
        let mut backing = vec![0xa5; capacity + 2];
        let mut bv = BitVec::new(&mut backing[..capacity]);
        let mut bits: Vec<bool> = (0..capacity * 8)
            .map(|i| matches!(i % 8, 0 | 2 | 5 | 7))
            .collect();
        let mut cursor = 0;
        let mut random = 0x3141_5926u32;
        for step in 0..512 {
            random = random.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match step % 5 {
                0 => {
                    cursor = random as usize % (bits.len() + 3);
                    bv.cur_bit = cursor;
                }
                1 => {
                    let count = (random % 11) as usize;
                    let input: Vec<_> = (0..count).map(|i| values[(i + step) % 4]).collect();
                    let mut written = 0;
                    while written < count && cursor < bits.len() {
                        bits[cursor] = match input[written] {
                            BitValue::Zero => false,
                            BitValue::One => true,
                            BitValue::L => padding[cursor % 8],
                            BitValue::H => !padding[cursor % 8],
                        };
                        written += 1;
                        cursor += 1;
                    }
                    assert_eq!(
                        bv.write_bits(&input),
                        if written == count {
                            Ok(())
                        } else {
                            Err(BitVecError::OutOfRange)
                        }
                    );
                }
                2 => {
                    let position = random as usize % (bits.len() + 3);
                    let value = values[step % 4];
                    let expected = if let Some(bit) = bits.get_mut(position) {
                        *bit = match value {
                            BitValue::Zero => false,
                            BitValue::One => true,
                            BitValue::L => padding[position % 8],
                            BitValue::H => !padding[position % 8],
                        };
                        Ok(())
                    } else {
                        Err(BitVecError::OutOfRange)
                    };
                    assert_eq!(bv.set(position, value), expected);
                }
                3 => {
                    let expected = if let Some(&bit) = bits.get(cursor) {
                        let value = if bit == padding[cursor % 8] {
                            BitValue::L
                        } else {
                            BitValue::H
                        };
                        cursor += 1;
                        Ok(value)
                    } else {
                        Err(BitVecError::OutOfRange)
                    };
                    assert_eq!(bv.read_padding_bit(), expected);
                }
                _ if step % 25 == 24 => {
                    bv.zero();
                    bits.fill(false);
                    cursor = 0;
                }
                _ => {}
            }
            assert_eq!(bv.cur_bit, cursor, "capacity {capacity}, step {step}");
            for (position, &bit) in bits.iter().enumerate() {
                assert_eq!(
                    bv.get(position),
                    Ok(if bit { BitValue::One } else { BitValue::Zero })
                );
            }
            assert_eq!(
                bv.remaining_bits(),
                bits.len()
                    .checked_sub(cursor)
                    .ok_or(BitVecError::OutOfRange)
            );
        }
        assert_eq!(&backing[capacity..], &[0xa5; 2]);
    }
}

#[test]
fn base64_round_trips_all_byte_values_and_preserves_destination_suffixes() {
    for len in 0..=256 {
        let input: Vec<_> = (0..len).map(|i| i as u8).collect();
        let encoded = base64::encode(&input).unwrap();
        let mut output = vec![0x7e; len + 3];
        assert_eq!(
            base64::decode_into(&mut output, encoded.as_bytes()),
            Ok(len)
        );
        assert_eq!(&output[..len], input);
        assert_eq!(&output[len..], &[0x7e; 3]);
        assert_eq!(base64::decode(encoded.as_bytes()), Ok(input));
    }
}

// Polynomial division over bytes is independent of the table-driven runtime.
fn reflected_crc(mut state: u16, input: &[u8], polynomial: u16) -> u16 {
    for &byte in input {
        state ^= u16::from(byte);
        for _ in 0..8 {
            let odd = state & 1 != 0;
            state /= 2;
            if odd {
                state ^= polynomial;
            }
        }
    }
    state
}
#[test]
fn crc_streaming_matches_bitwise_division_at_every_split() {
    let input: Vec<_> = (0..=255).collect();
    for initial in [0, 1, 0x1234, 0xffff] {
        for (compute, polynomial) in [
            (crc16 as fn(u16, &[u8]) -> u16, 0xa001),
            (crc16_ccitt, 0x8408),
        ] {
            let expected = reflected_crc(initial, &input, polynomial);
            assert_eq!(compute(initial, &input), expected);
            for split in 0..=input.len() {
                let mid = compute(initial, &input[..split]);
                assert_eq!(mid, reflected_crc(initial, &input[..split], polynomial));
                assert_eq!(compute(mid, &input[split..]), expected);
            }
        }
    }
}
