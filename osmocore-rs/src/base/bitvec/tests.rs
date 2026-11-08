use super::legacy::*;
use super::*;

#[test]
fn short_adapter_buffers_and_invalid_format_cursors_fail_atomically() {
    let mut data = [0xa5, 0x5a];
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = 9;
    let mut output = [0x7e];
    assert_eq!(bitvec_pack(&bv, &mut output), Err(BitVecError::OutOfRange));
    assert_eq!(bitvec_unpack(&mut bv, &[0]), Err(BitVecError::OutOfRange));
    assert_eq!(
        bitvec_to_string_r(&bv, &mut output),
        Err(BitVecError::OutOfRange)
    );
    // C's documented cur_bit+1 capacity omits byte separator spaces.
    let mut output = [0x7e; 10];
    assert_eq!(
        bitvec_to_string_r(&bv, &mut output),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(output, [0x7e; 10]);
    bv.cur_bit = usize::MAX;
    let mut output = [0x7e; 32];
    assert_eq!(
        bitvec_to_string_r(&bv, &mut output),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(output, [0x7e; 32]);
    assert_eq!(bv.cur_bit, usize::MAX);
    assert_eq!(bv.data(), [0xa5, 0x5a]);
}

#[test]
fn empty_adapter_slices_and_rust_terminated_hex_are_safe() {
    let mut bv = BitVec::new(&mut []);
    bv.cur_bit = usize::MAX;
    assert_eq!(bitvec_pack(&bv, &mut []), Ok(0));
    assert_eq!(bitvec_unpack(&mut bv, &[]), Ok(0));
    assert_eq!(bv.cur_bit, usize::MAX);
    assert_eq!(bitvec_unhex(&mut bv, &[]), 0);
    assert_eq!(bv.cur_bit, 0);
    assert_eq!(
        bitvec_to_string_r(&bv, &mut []),
        Err(BitVecError::OutOfRange)
    );
    let mut output = [0x7e; 2];
    assert_eq!(bitvec_to_string_r(&bv, &mut output), Ok(()));
    assert_eq!(output, [0, 0x7e]);

    let mut data = [0xa5; 2];
    let mut bv = BitVec::new(&mut data);
    assert_eq!(bitvec_unhex(&mut bv, b"a1b2"), 0);
    assert_eq!(bv.cur_bit, 16);
    assert_eq!(bv.data(), [0xa1, 0xb2]);
}

#[test]
fn wrapped_field_spans_and_undefined_shifts_do_not_access_storage() {
    let mut data = [0x80];
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = 3;
    let mut index = u32::MAX;
    assert_eq!(
        bitvec_read_field(&mut bv, &mut index, 2),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!((bv.cur_bit, index), (3, u32::MAX));
    assert_eq!(bv.data(), [0x80]);

    let mut data = [0u8; 9];
    data[0] = 0x80;
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = 7;
    let mut index = 0;
    assert_eq!(
        bitvec_read_field(&mut bv, &mut index, 65),
        Err(BitVecError::TooManyBits)
    );
    assert_eq!((bv.cur_bit, index), (0, 0));
    assert_eq!(bv.data()[0], 0x80);
}

#[test]
fn unsafe_shift_and_run_spans_return_errors() {
    let mut data = [0xa5];
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = 8;
    assert_eq!(bitvec_shiftl(&mut bv, 1), Err(BitVecError::OutOfRange));
    assert_eq!(bv.cur_bit, 8);
    assert_eq!(bv.data(), [0xa5]);

    let mut data = [0xa5, 0x5a];
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = 32;
    assert_eq!(bitvec_shiftl(&mut bv, 24), Err(BitVecError::OutOfRange));
    assert_eq!(bv.cur_bit, 32);
    assert_eq!(bv.data(), [0xa5, 0x5a]);

    let mut bv = BitVec::new(&mut []);
    bv.cur_bit = 1;
    assert_eq!(bitvec_rl(&bv, false), Err(BitVecError::OutOfRange));
    assert_eq!(
        bitvec_rl_curbit(&mut bv, false, 8),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(bv.cur_bit, 1);
}

#[test]
fn run_outcomes_preserve_cursor_including_partial_storage_failures() {
    for (bytes, start, one, limit, result, cursor) in [
        (vec![], 1, false, 8, Err(BitVecError::OutOfRange), 1),
        (vec![], 1, true, 8, Ok(0), 0),
        (vec![0], 1, false, 16, Err(BitVecError::OutOfRange), 8),
        (vec![0], 1, false, 4, Ok(3), 3),
        (vec![0, 0xff], 1, false, 9, Ok(7), 8),
        (vec![0, 0], 1, false, 16, Ok(15), 16),
        (vec![0, 0, 0x10], 8, false, 17, Ok(11), 19),
        (vec![], 16, false, 8, Ok(u32::MAX - 7), 8),
        (vec![], 1, false, 1, Ok(0), 0),
    ] {
        let mut storage = bytes.clone();
        let mut bv = BitVec::new(&mut storage);
        bv.cur_bit = start;
        assert_eq!(bitvec_rl_curbit(&mut bv, one, limit), result);
        assert_eq!(bv.cur_bit, cursor);
        assert_eq!(bv.data(), bytes);
    }
}

#[test]
fn signed_numeric_results_remain_successful() {
    let mut data = [0xff, 0xff, 0xff, 0xea];
    let mut bv = BitVec::new(&mut data);
    assert_eq!(bitvec_get_uint(&mut bv, 32), Ok(-22));
    assert_eq!(bv.cur_bit, 32);
    assert_eq!(bitvec_get_uint(&mut bv, 1), Err(BitVecError::OutOfRange));
    assert_eq!(bv.cur_bit, 32);
}

#[test]
fn undefined_numeric_shifts_and_short_scalar_storage_fail_safely() {
    let mut data = [0x40];
    let mut bv = BitVec::new(&mut data);
    // First bit is zero; the next one would require a shift by 32 in C.
    assert_eq!(bitvec_get_uint(&mut bv, 34), Err(BitVecError::TooManyBits));
    assert_eq!(bv.cur_bit, 1);
    bv.cur_bit = 16;
    assert_eq!(bitvec_get_int16_msb(&bv, 9), Err(BitVecError::OutOfRange));
    assert_eq!(bv.cur_bit, 16);
    assert_eq!(
        bitvec_set_u64(&mut bv, u64::MAX, 65, true),
        Err(BitVecError::TooManyBits)
    );
    assert_eq!(bv.data(), [0x40]);
    assert_eq!(bv.cur_bit, 16);
}

#[test]
fn rust_only_positions_and_byte_arithmetic_fail_without_writes() {
    let mut data = [0xa5];
    let mut dst = [0x5a];
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = 8;
    assert_eq!(
        byte_span(&bv, usize::MAX),
        Err(BitVecError::ArithmeticOverflow)
    );
    bv.cur_bit = usize::MAX;
    assert_eq!(
        bitvec_get_bytes(&mut bv, &mut dst),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(
        bitvec_set_bytes(&mut bv, &[0]),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(bitvec_get_uint(&mut bv, 0), Ok(0));
    assert_eq!(bv.cur_bit, usize::MAX);
    assert_eq!(bv.data(), [0xa5]);
    assert_eq!(dst, [0x5a]);
    if usize::BITS > 32 {
        bv.cur_bit = (u32::MAX as usize).checked_add(1).unwrap();
        assert_eq!(bitvec_tailroom_bits(&bv), 8);
        assert_eq!(
            bitvec_tailroom_bits_checked(&bv),
            Err(BitVecError::OutOfRange)
        );
        assert_eq!(
            bitvec_fill(&mut bv, 1, BitValue::One),
            Err(BitVecError::ArithmeticOverflow)
        );
    }
}

#[test]
fn extreme_positions_fail_without_mutation_or_cursor_progress() {
    let mut data = [0xa5];
    let mut bv = BitVec::new(&mut data);
    bv.cur_bit = usize::MAX;
    assert_eq!(
        bitvec_set_bit(&mut bv, BitValue::H),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(bitvec_get_bit_high(&mut bv), Err(BitVecError::OutOfRange));
    assert_eq!(
        bitvec_get_bit_pos(&bv, usize::MAX),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(
        bitvec_set_bit_pos(&mut bv, usize::MAX, BitValue::One),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(bitvec_tailroom_bits(&bv), 9);
    assert_eq!(
        bitvec_tailroom_bits_checked(&bv),
        Err(BitVecError::OutOfRange)
    );
    assert_eq!(bitvec_used_bytes(&bv), usize::MAX / 8 + 1);
    assert_eq!(bitvec_set_bits(&mut bv, &[]), Ok(()));
    assert_eq!(bv.cur_bit, usize::MAX);
    assert_eq!(bv.data(), [0xa5]);
}

#[test]
fn empty_storage_supports_zero_and_empty_writes() {
    let mut bv = BitVec::new(&mut []);
    assert_eq!(bitvec_tailroom_bits(&bv), 0);
    assert_eq!(bitvec_set_bits(&mut bv, &[]), Ok(()));
    assert_eq!(bitvec_get_bit_high(&mut bv), Err(BitVecError::OutOfRange));
    bv.cur_bit = usize::MAX;
    bitvec_zero(&mut bv);
    assert_eq!(bv.cur_bit, 0);
    assert_eq!(bitvec_used_bytes(&bv), 0);
}
