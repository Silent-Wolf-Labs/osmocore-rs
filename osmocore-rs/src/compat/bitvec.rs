// SPDX-License-Identifier: GPL-3.0-or-later

pub use crate::bitvec::legacy::{
    bitvec_add_array, bitvec_fill, bitvec_find_bit_pos, bitvec_get_bit_high, bitvec_get_bit_pos,
    bitvec_get_bit_pos_high, bitvec_get_bytes, bitvec_get_int16_msb, bitvec_get_nth_set_bit,
    bitvec_get_uint, bitvec_pack, bitvec_read_field, bitvec_rl, bitvec_rl_curbit, bitvec_set_bit,
    bitvec_set_bit_pos, bitvec_set_bits, bitvec_set_bytes, bitvec_set_u64, bitvec_set_uint,
    bitvec_shiftl, bitvec_spare_padding, bitvec_tailroom_bits, bitvec_tailroom_bits_checked,
    bitvec_to_string_r, bitvec_unhex, bitvec_unpack, bitvec_used_bytes, bitvec_write_field,
    bitvec_zero,
};
