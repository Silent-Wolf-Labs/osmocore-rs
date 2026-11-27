// SPDX-License-Identifier: GPL-3.0-or-later

//! Speech codec frames, the counterpart of libosmocodec's `src/codec`: the frame sizes
//! and bit orders that GSM 05.03 channel coding needs for full rate ([`GSM610_BITORDER`])
//! and enhanced full rate ([`GSM660_BITORDER`]).
//!
//! ```
//! use osmocore_rs::codec::{GSM610_BITORDER, GSM_FR_BYTES};
//! assert_eq!(GSM_FR_BYTES, 33);
//! // Channel coding takes serial bit 0 first.
//! assert_eq!(GSM610_BITORDER[0], 0);
//! ```

mod gsm610;
mod gsm660;

pub use gsm610::{GSM_FR_BYTES, GSM610_BITORDER};
pub use gsm660::{GSM_EFR_BYTES, GSM660_BITORDER};
