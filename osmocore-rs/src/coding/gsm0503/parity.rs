// SPDX-License-Identifier: GPL-3.0-or-later

//! The parity codes of GSM 05.03. Each polynomial omits its leading term.

use crate::base::crc::{Crc8Code, Crc16Code, Crc64Code};

/// Protects an xCCH block: the Fire code g(D) = (D^23 + 1)(D^17 + D^3 + 1), with
/// inverted parity.
pub const FIRE_CRC40: Crc64Code = Crc64Code {
    bits: 40,
    poly: 0x00_0482_0009,
    init: 0,
    remainder: 0xff_ffff_ffff,
};

/// Protects a CS-2, CS-3 or CS-4 block: g(D) = D^16 + D^12 + D^5 + 1, with inverted
/// parity.
pub const CS234_CRC16: Crc16Code = Crc16Code {
    bits: 16,
    poly: 0x1021,
    init: 0,
    remainder: 0xffff,
};

/// Protects an EGPRS header: g(D) = D^8 + D^6 + D^3 + 1, with inverted parity.
pub const MCS_CRC8_HDR: Crc8Code = Crc8Code {
    bits: 8,
    poly: 0x49,
    init: 0,
    remainder: 0xff,
};

/// Protects EGPRS data: g(D) = D^12 + D^11 + D^10 + D^8 + D^5 + D^4 + 1, with inverted
/// parity.
pub const MCS_CRC12: Crc16Code = Crc16Code {
    bits: 12,
    poly: 0xd31,
    init: 0,
    remainder: 0xfff,
};

/// Protects a RACH burst: g(D) = D^6 + D^5 + D^3 + D^2 + D + 1, with inverted parity.
pub const RACH_CRC6: Crc8Code = Crc8Code {
    bits: 6,
    poly: 0x2f,
    init: 0,
    remainder: 0x3f,
};

/// Protects an SCH burst: g(D) = D^10 + D^8 + D^6 + D^5 + D^4 + D^2 + 1, with inverted
/// parity.
pub const SCH_CRC10: Crc16Code = Crc16Code {
    bits: 10,
    poly: 0x175,
    init: 0,
    remainder: 0x3ff,
};

/// Protects the class 1a bits of a TCH/FS frame: g(D) = D^3 + D + 1, with inverted
/// parity.
pub const TCH_FR_CRC3: Crc8Code = Crc8Code {
    bits: 3,
    poly: 0x3,
    init: 0,
    remainder: 0x7,
};

/// Protects the selected bits of a TCH/EFS frame: g(D) = D^8 + D^4 + D^3 + D^2 + 1.
pub const TCH_EFR_CRC8: Crc8Code = Crc8Code {
    bits: 8,
    poly: 0x1d,
    init: 0,
    remainder: 0,
};

/// Protects the class 1a bits of an AMR frame: g(D) = D^6 + D^5 + D^3 + D^2 + D + 1,
/// with inverted parity.
pub const AMR_CRC6: Crc8Code = Crc8Code {
    bits: 6,
    poly: 0x2f,
    init: 0,
    remainder: 0x3f,
};

/// Protects an AMR SID_UPDATE frame: g(D) = D^14 + D^13 + D^5 + D^3 + D^2 + 1, with
/// inverted parity.
pub const AMR_CRC14: Crc16Code = Crc16Code {
    bits: 14,
    poly: 0x202d,
    init: 0,
    remainder: 0x3fff,
};
