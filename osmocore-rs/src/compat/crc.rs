// SPDX-License-Identifier: GPL-3.0-or-later

/// Recorded C metadata.
pub const OSMO_CRC16_TABLE: [u16; 256] = crate::crc::OSMO_CRC16_TABLE;

/// Recorded C metadata.
pub const OSMO_CRC16_CCITT_TABLE: [u16; 256] = crate::crc::OSMO_CRC16_CCITT_TABLE;

pub use crate::crc::legacy::{
    osmo_crc8gen_check_bits, osmo_crc8gen_compute_bits, osmo_crc8gen_set_bits,
};
pub use crate::crc::legacy::{
    osmo_crc16, osmo_crc16_byte, osmo_crc16_ccitt, osmo_crc16_ccitt_byte,
};
pub use crate::crc::legacy::{
    osmo_crc16gen_check_bits, osmo_crc16gen_compute_bits, osmo_crc16gen_set_bits,
};
pub use crate::crc::legacy::{
    osmo_crc32gen_check_bits, osmo_crc32gen_compute_bits, osmo_crc32gen_set_bits,
};
pub use crate::crc::legacy::{
    osmo_crc64gen_check_bits, osmo_crc64gen_compute_bits, osmo_crc64gen_set_bits,
};
