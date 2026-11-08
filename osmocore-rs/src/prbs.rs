// SPDX-License-Identifier: GPL-3.0-or-later

//! Deterministic pseudo-random bit sequences from a linear-feedback shift register.
//!
//! The generator starts from state 1 and always produces the same bits, so a
//! test can regenerate a pattern instead of storing it. Bits are unpacked: each
//! output byte is 0 or 1, as in [`bits`](crate::bits).
//!
//! ```
//! use osmocore_rs::prbs::{PRBS7, PrbsState};
//! let mut generator = PrbsState::new(PRBS7);
//! assert_eq!(generator.next_bit(), 1);
//! let mut bits = [0u8; 4];
//! generator.fill(&mut bits);
//! assert_eq!(bits, [0, 0, 0, 0]);
//! ```

/// A sequence definition. Callers can define their own: [`len`](Self::len) only
/// describes the sequence, and the generator takes its behavior from
/// [`coeff`](Self::coeff) alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prbs {
    /// Human-readable name.
    pub name: &'static str,
    /// Register length in bits.
    pub len: u32,
    /// Feedback mask, XORed into the register after each output bit of 1.
    pub coeff: u64,
}

/// PRBS-7 from ITU-T O.150, x^7 + x^6 + 1, repeating every 127 bits.
pub const PRBS7: Prbs = Prbs {
    name: "PRBS-7",
    len: 7,
    coeff: 0x60,
};
/// PRBS-9 from ITU-T O.150, x^9 + x^5 + 1, repeating every 511 bits.
pub const PRBS9: Prbs = Prbs {
    name: "PRBS-9",
    len: 9,
    coeff: 0x110,
};
/// PRBS-11 from ITU-T O.150, x^11 + x^9 + 1, repeating every 2,047 bits.
pub const PRBS11: Prbs = Prbs {
    name: "PRBS-11",
    len: 11,
    coeff: 0x500,
};
/// PRBS-15 from ITU-T O.150, x^15 + x^14 + 1, repeating every 32,767 bits.
pub const PRBS15: Prbs = Prbs {
    name: "PRBS-15",
    len: 15,
    coeff: 0x6000,
};

/// A running generator. Clone it to replay the sequence from the current point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrbsState {
    prbs: Prbs,
    state: u64,
}

impl PrbsState {
    /// Start `prbs` from state 1.
    pub const fn new(prbs: Prbs) -> Self {
        Self { prbs, state: 1 }
    }

    /// The sequence this generator produces.
    pub const fn prbs(&self) -> Prbs {
        self.prbs
    }

    /// The current register value.
    pub const fn state(&self) -> u64 {
        self.state
    }

    /// Return the next bit, 0 or 1.
    pub const fn next_bit(&mut self) -> u8 {
        let bit = self.state & 1;
        self.state >>= 1;
        if bit != 0 {
            self.state ^= self.prbs.coeff;
        }
        bit as u8
    }

    /// Return the next `count` bits as one number, the first bit most significant. A
    /// count of 0 returns 0.
    ///
    /// # Panics
    ///
    /// If `count` is more than 64.
    pub const fn next_bits(&mut self, count: u32) -> u64 {
        assert!(count <= 64, "count must be at most 64");
        let mut value = 0u64;
        let mut taken = 0;
        while taken < count {
            value = (value << 1) | self.next_bit() as u64;
            taken += 1;
        }
        value
    }

    /// Overwrite all of `out` with the next unpacked bits.
    pub fn fill(&mut self, out: &mut [u8]) {
        for bit in out {
            *bit = self.next_bit();
        }
    }
}
