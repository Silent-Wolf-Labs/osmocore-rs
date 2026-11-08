// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM utilities, the counterpart of libosmocore's `src/gsm`: TDMA frames and paging
//! ([`gsm0502`]), bands, power, time and 7-bit coding ([`gsm_utils`]), A5 ciphers
//! ([`a5`]) and TLV messages ([`tlv`]).

pub mod a5;
pub mod gsm0502;
pub mod gsm_utils;
pub mod tlv;
