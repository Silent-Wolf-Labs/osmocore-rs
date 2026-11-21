// SPDX-License-Identifier: GPL-3.0-or-later

//! The convolutional codes of GSM 05.03, built from their generator polynomials.

use super::tables;
use crate::base::conv::{ConvCode, ConvCodeSpec, Termination};
use std::sync::OnceLock;

// Generator polynomials from 3GPP TS 45.003 annex B. Bit i is the coefficient of D^i.
/// 1 + D^3 + D^4
const G0: u8 = 0b001_1001;
/// 1 + D + D^3 + D^4
const G1: u8 = 0b001_1011;
/// 1 + D^2 + D^4
const G2: u8 = 0b001_0101;
/// 1 + D + D^2 + D^3 + D^4
const G3: u8 = 0b001_1111;
/// 1 + D^2 + D^3 + D^5 + D^6
const G4: u8 = 0b110_1101;
/// 1 + D + D^4 + D^6
const G5: u8 = 0b101_0011;
/// 1 + D + D^2 + D^3 + D^4 + D^6
const G6: u8 = 0b101_1111;
/// 1 + D + D^2 + D^3 + D^6
const G7: u8 = 0b100_1111;

/// One output of a code, in the order the output symbol carries them, first one in
/// the most significant bit.
#[derive(Clone, Copy)]
enum Output {
    /// The input bit itself, in a recursive code.
    Systematic,
    /// The parity of the register under this polynomial. In a recursive code, the
    /// register holds the fed-back bits, so this divides by the feedback polynomial.
    Parity(u8),
}

use Output::{Parity, Systematic};

const XCCH: &[Output] = &[Parity(G0), Parity(G1)];
const RATE_1_3: &[Output] = &[Parity(G1), Parity(G2), Parity(G3)];
const MCS: &[Output] = &[Parity(G4), Parity(G7), Parity(G5)];

struct Definition {
    len: u16,
    k: u8,
    outputs: &'static [Output],
    feedback: Option<u8>,
    tail_biting: bool,
    puncture: Option<fn() -> Vec<i32>>,
}

impl Definition {
    const fn new(len: u16, k: u8, outputs: &'static [Output]) -> Self {
        Self {
            len,
            k,
            outputs,
            feedback: None,
            tail_biting: false,
            puncture: None,
        }
    }

    const fn recursive(len: u16, k: u8, feedback: u8, outputs: &'static [Output]) -> Self {
        Self {
            feedback: Some(feedback),
            ..Self::new(len, k, outputs)
        }
    }

    const fn punctured(self, puncture: fn() -> Vec<i32>) -> Self {
        Self {
            puncture: Some(puncture),
            ..self
        }
    }

    const fn tail_biting(self) -> Self {
        Self {
            tail_biting: true,
            ..self
        }
    }

    /// Returns the output symbol for a register holding `window`, newest bit lowest.
    fn symbol(&self, window: u32, input: u8) -> u8 {
        self.outputs.iter().fold(0, |symbol, output| {
            symbol << 1
                | match output {
                    Systematic => input,
                    Parity(polynomial) => parity(window & u32::from(*polynomial)),
                }
        })
    }

    fn build(&self) -> ConvCode {
        let states = 1u32 << (self.k - 1);
        let mask = states - 1;
        let (next_output, next_state) = (0..states)
            .map(|state| {
                let step = |input: u8| {
                    let register = match self.feedback {
                        None => input,
                        Some(feedback) => {
                            parity((state << 1 | u32::from(input)) & u32::from(feedback))
                        }
                    };
                    let window = state << 1 | u32::from(register);
                    (self.symbol(window, input), (window & mask) as u8)
                };
                let (zero, one) = (step(0), step(1));
                ([zero.0, one.0], [zero.1, one.1])
            })
            .unzip();
        // A recursive code terminates by feeding zeros into its register, which takes
        // the input that cancels the feedback.
        let (next_term_output, next_term_state) = match self.feedback {
            None => (None, None),
            Some(feedback) => {
                let (output, state) = (0..states)
                    .map(|state| {
                        let window = state << 1;
                        let input = parity(window & u32::from(feedback));
                        (self.symbol(window, input), (window & mask) as u8)
                    })
                    .unzip();
                (Some(output), Some(state))
            }
        };
        ConvCode::new(ConvCodeSpec {
            n: self.outputs.len() as i32,
            k: i32::from(self.k),
            len: i32::from(self.len),
            term: if self.tail_biting {
                Termination::TAIL_BITING
            } else {
                Termination::FLUSH
            },
            next_output,
            next_state,
            next_term_output,
            next_term_state,
            puncture: self.puncture.map(|puncture| puncture()),
        })
        .expect("the GSM 05.03 code tables are valid")
    }
}

fn parity(bits: u32) -> u8 {
    (bits.count_ones() & 1) as u8
}

/// Converts a listed puncturing pattern.
macro_rules! listed {
    ($table:ident) => {
        || tables::$table.map(i32::from).to_vec()
    };
}

macro_rules! codes {
    ($($(#[$doc:meta])* $variant:ident = $c_name:literal, $definition:expr;)*) => {
        /// A convolutional code of GSM 05.03.
        ///
        /// [`code`](Self::code) gives the descriptor, built once on first use. The
        /// variants follow libosmocore's `gsm0503_*` codes, named by
        /// [`c_name`](Self::c_name).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Gsm0503Code {
            $($(#[$doc])* $variant,)*
        }

        impl Gsm0503Code {
            /// Every code, in libosmocore's order.
            pub const ALL: [Self; 45] = [$(Self::$variant),*];

            /// Returns the name of libosmocore's descriptor for this code.
            pub const fn c_name(self) -> &'static str {
                match self {
                    $(Self::$variant => $c_name,)*
                }
            }

            fn definition(self) -> Definition {
                match self {
                    $(Self::$variant => $definition,)*
                }
            }
        }
    };
}

codes! {
    /// SACCH, SDCCH, BCCH, PCH, AGCH, FACCH and CS-1: rate 1/2, K = 5.
    Xcch = "gsm0503_xcch", Definition::new(224, 5, XCCH);
    /// TCH/F2.4: rate 1/6, K = 5.
    TchF24 = "gsm0503_tch_f24", Definition::new(
        72,
        5,
        &[Parity(G1), Parity(G2), Parity(G3), Parity(G1), Parity(G2), Parity(G3)],
    );
    /// TCH/H2.4: rate 1/3, K = 5.
    TchH24 = "gsm0503_tch_h24", Definition::new(72, 5, RATE_1_3);
    /// TCH/F4.8: rate 1/3, K = 5.
    TchF48 = "gsm0503_tch_f48", Definition::new(148, 5, RATE_1_3);
    /// TCH/F9.6 and TCH/H4.8: rate 1/2, K = 5, punctured.
    TchF96 = "gsm0503_tch_f96", Definition::new(240, 5, XCCH).punctured(tables::tch_f96);
    /// TCH/F14.4: rate 1/2, K = 5, punctured.
    TchF144 = "gsm0503_tch_f144", Definition::new(290, 5, XCCH).punctured(tables::tch_f144);
    /// RACH with an 8-bit RA: rate 1/2, K = 5.
    Rach = "gsm0503_rach", Definition::new(14, 5, XCCH);
    /// RACH with an 11-bit RA: rate 1/2, K = 5, punctured.
    RachExt = "gsm0503_rach_ext", Definition::new(17, 5, XCCH).punctured(listed!(RACH_EXT));
    /// SCH: rate 1/2, K = 5.
    Sch = "gsm0503_sch", Definition::new(35, 5, XCCH);
    /// CS-2: rate 1/2, K = 5, punctured.
    Cs2 = "gsm0503_cs2", Definition::new(290, 5, XCCH).punctured(tables::cs2);
    /// CS-3: rate 1/2, K = 5, punctured.
    Cs3 = "gsm0503_cs3", Definition::new(334, 5, XCCH).punctured(tables::cs3);
    /// CS-2 without puncturing.
    Cs2Np = "gsm0503_cs2_np", Definition::new(290, 5, XCCH);
    /// CS-3 without puncturing.
    Cs3Np = "gsm0503_cs3_np", Definition::new(334, 5, XCCH);
    /// TCH/AFS12.2: recursive, rate 1/2, K = 5, punctured.
    TchAfs12_2 = "gsm0503_tch_afs_12_2",
        Definition::recursive(250, 5, G0, &[Systematic, Parity(G1)])
            .punctured(listed!(TCH_AFS_12_2));
    /// TCH/AFS10.2: recursive, rate 1/3, K = 5, punctured.
    TchAfs10_2 = "gsm0503_tch_afs_10_2",
        Definition::recursive(210, 5, G3, &[Parity(G1), Parity(G2), Systematic])
            .punctured(listed!(TCH_AFS_10_2));
    /// TCH/AFS7.95: recursive, rate 1/3, K = 7, punctured.
    TchAfs7_95 = "gsm0503_tch_afs_7_95",
        Definition::recursive(165, 7, G4, &[Systematic, Parity(G5), Parity(G6)])
            .punctured(listed!(TCH_AFS_7_95));
    /// TCH/AFS7.4: recursive, rate 1/3, K = 5, punctured.
    TchAfs7_4 = "gsm0503_tch_afs_7_4",
        Definition::recursive(154, 5, G3, &[Parity(G1), Parity(G2), Systematic])
            .punctured(listed!(TCH_AFS_7_4));
    /// TCH/AFS6.7: recursive, rate 1/4, K = 5, punctured.
    TchAfs6_7 = "gsm0503_tch_afs_6_7",
        Definition::recursive(140, 5, G3, &[Parity(G1), Parity(G2), Systematic, Systematic])
            .punctured(listed!(TCH_AFS_6_7));
    /// TCH/AFS5.9: recursive, rate 1/4, K = 7, punctured.
    TchAfs5_9 = "gsm0503_tch_afs_5_9",
        Definition::recursive(124, 7, G6, &[Parity(G4), Parity(G5), Systematic, Systematic])
            .punctured(listed!(TCH_AFS_5_9));
    /// TCH/AFS5.15: recursive, rate 1/5, K = 5, punctured.
    TchAfs5_15 = "gsm0503_tch_afs_5_15",
        Definition::recursive(
            109,
            5,
            G3,
            &[Parity(G1), Parity(G1), Parity(G2), Systematic, Systematic],
        )
        .punctured(listed!(TCH_AFS_5_15));
    /// TCH/AFS4.75: recursive, rate 1/5, K = 7, punctured.
    TchAfs4_75 = "gsm0503_tch_afs_4_75",
        Definition::recursive(
            101,
            7,
            G6,
            &[Parity(G4), Parity(G4), Parity(G5), Systematic, Systematic],
        )
        .punctured(listed!(TCH_AFS_4_75));
    /// TCH/FS and TCH/EFS: rate 1/2, K = 5.
    TchFr = "gsm0503_tch_fr", Definition::new(185, 5, XCCH);
    /// TCH/HS: rate 1/3, K = 7, punctured.
    TchHr = "gsm0503_tch_hr",
        Definition::new(98, 7, &[Parity(G4), Parity(G5), Parity(G6)]).punctured(tables::tch_hr);
    /// TCH/AHS7.95: recursive, rate 1/2, K = 5, punctured.
    TchAhs7_95 = "gsm0503_tch_ahs_7_95",
        Definition::recursive(129, 5, G0, &[Systematic, Parity(G1)])
            .punctured(listed!(TCH_AHS_7_95));
    /// TCH/AHS7.4: recursive, rate 1/2, K = 5, punctured.
    TchAhs7_4 = "gsm0503_tch_ahs_7_4",
        Definition::recursive(126, 5, G0, &[Systematic, Parity(G1)])
            .punctured(listed!(TCH_AHS_7_4));
    /// TCH/AHS6.7: recursive, rate 1/2, K = 5, punctured.
    TchAhs6_7 = "gsm0503_tch_ahs_6_7",
        Definition::recursive(116, 5, G0, &[Systematic, Parity(G1)])
            .punctured(listed!(TCH_AHS_6_7));
    /// TCH/AHS5.9: recursive, rate 1/2, K = 5, punctured.
    TchAhs5_9 = "gsm0503_tch_ahs_5_9",
        Definition::recursive(108, 5, G0, &[Systematic, Parity(G1)])
            .punctured(listed!(TCH_AHS_5_9));
    /// TCH/AHS5.15: recursive, rate 1/3, K = 5, punctured.
    TchAhs5_15 = "gsm0503_tch_ahs_5_15",
        Definition::recursive(97, 5, G3, &[Parity(G1), Parity(G2), Systematic])
            .punctured(listed!(TCH_AHS_5_15));
    /// TCH/AHS4.75: recursive, rate 1/3, K = 7, punctured.
    TchAhs4_75 = "gsm0503_tch_ahs_4_75",
        Definition::recursive(89, 7, G4, &[Systematic, Parity(G5), Parity(G6)])
            .punctured(listed!(TCH_AHS_4_75));
    /// AMR SID_UPDATE on TCH/AFS and TCH/AHS: recursive, rate 1/4, K = 5.
    TchAxsSidUpdate = "gsm0503_tch_axs_sid_update",
        Definition::recursive(49, 5, G3, &[Parity(G1), Parity(G2), Systematic, Systematic]);
    /// EGPRS MCS-1 to MCS-4 downlink header: rate 1/3, K = 7, tail-biting.
    Mcs1DlHdr = "gsm0503_mcs1_dl_hdr", Definition::new(36, 7, MCS).tail_biting();
    /// EGPRS MCS-1 to MCS-4 uplink header: rate 1/3, K = 7, tail-biting.
    Mcs1UlHdr = "gsm0503_mcs1_ul_hdr", Definition::new(39, 7, MCS).tail_biting();
    /// EGPRS MCS-1 data: rate 1/3, K = 7.
    Mcs1 = "gsm0503_mcs1", Definition::new(190, 7, MCS);
    /// EGPRS MCS-2 data: rate 1/3, K = 7.
    Mcs2 = "gsm0503_mcs2", Definition::new(238, 7, MCS);
    /// EGPRS MCS-3 data: rate 1/3, K = 7.
    Mcs3 = "gsm0503_mcs3", Definition::new(310, 7, MCS);
    /// EGPRS MCS-4 data: rate 1/3, K = 7.
    Mcs4 = "gsm0503_mcs4", Definition::new(366, 7, MCS);
    /// EGPRS MCS-5 and MCS-6 downlink header: rate 1/3, K = 7, tail-biting.
    Mcs5DlHdr = "gsm0503_mcs5_dl_hdr", Definition::new(33, 7, MCS).tail_biting();
    /// EGPRS MCS-5 and MCS-6 uplink header: rate 1/3, K = 7, tail-biting.
    Mcs5UlHdr = "gsm0503_mcs5_ul_hdr", Definition::new(45, 7, MCS).tail_biting();
    /// EGPRS MCS-5 data: rate 1/3, K = 7.
    Mcs5 = "gsm0503_mcs5", Definition::new(462, 7, MCS);
    /// EGPRS MCS-6 data: rate 1/3, K = 7.
    Mcs6 = "gsm0503_mcs6", Definition::new(606, 7, MCS);
    /// EGPRS MCS-7 to MCS-9 downlink header: rate 1/3, K = 7, tail-biting.
    Mcs7DlHdr = "gsm0503_mcs7_dl_hdr", Definition::new(45, 7, MCS).tail_biting();
    /// EGPRS MCS-7 to MCS-9 uplink header: rate 1/3, K = 7, tail-biting.
    Mcs7UlHdr = "gsm0503_mcs7_ul_hdr", Definition::new(54, 7, MCS).tail_biting();
    /// EGPRS MCS-7 data: rate 1/3, K = 7.
    Mcs7 = "gsm0503_mcs7", Definition::new(462, 7, MCS);
    /// EGPRS MCS-8 data: rate 1/3, K = 7.
    Mcs8 = "gsm0503_mcs8", Definition::new(558, 7, MCS);
    /// EGPRS MCS-9 data: rate 1/3, K = 7.
    Mcs9 = "gsm0503_mcs9", Definition::new(606, 7, MCS);
}

impl Gsm0503Code {
    /// Returns the code's descriptor, built on first use and shared afterwards.
    pub fn code(self) -> &'static ConvCode {
        static CODES: [OnceLock<ConvCode>; Gsm0503Code::ALL.len()] =
            [const { OnceLock::new() }; Gsm0503Code::ALL.len()];
        CODES[self as usize].get_or_init(|| self.definition().build())
    }
}
