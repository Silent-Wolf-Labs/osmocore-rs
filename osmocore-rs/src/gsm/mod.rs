// SPDX-License-Identifier: GPL-3.0-or-later

//! GSM utilities, the counterpart of libosmocore's `src/gsm`: TDMA frames and paging
//! ([`gsm0502`]), bands, power, time and 7-bit coding ([`gsm_utils`]), A5 and GEA
//! ciphers on KASUMI ([`a5`], [`gea`] and [`kasumi`]), GSM and UMTS authentication
//! ([`auth`], [`comp128`] and, with the `auth` feature, `milenage`) and TLV messages
//! ([`tlv`]).

pub mod a5;
pub mod auth;
pub mod comp128;
pub mod gea;
pub mod gsm0502;
pub mod gsm_utils;
pub mod kasumi;
#[cfg(feature = "auth")]
pub mod milenage;
pub mod tlv;
