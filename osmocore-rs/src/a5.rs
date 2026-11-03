// SPDX-License-Identifier: GPL-3.0-or-later

//! The GSM A5/1 and A5/2 cipher streams, and the empty A5/0.
//!
//! Each call gives one frame's stream for each direction, as unpacked bits to XOR with
//! a burst's 114 data bits. A5/1 and A5/2 are publicly broken: use them to interoperate
//! with GSM networks, not to protect data.
//!
//! ```
//! use osmocore_rs::a5::a5_1;
//! let stream = a5_1(&[0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef], 123456);
//! assert_eq!(stream.downlink[..8], [1, 1, 0, 0, 1, 0, 1, 1]);
//! assert_eq!(stream.uplink[..8], [1, 1, 0, 1, 1, 0, 0, 1]);
//! ```

/// Unpacked bits in each direction of a stream.
pub const STREAM_BITS: usize = 114;
/// Bytes in a key.
pub const KEY_BYTES: usize = 8;

const FRAME_COUNT_BITS: u32 = 22;
const REGISTER_LENGTHS: [u32; 4] = [19, 22, 23, 17];
const REGISTER_TAPS: [u32; 4] = [0x07_2000, 0x30_0000, 0x70_0080, 0x01_0800];
/// Each register's clocking bit in A5/1.
const A5_1_CLOCK_BITS: [u32; 3] = [1 << 8, 1 << 10, 1 << 10];
/// The bits of the fourth register that clock the others in A5/2.
const A5_2_CLOCK_BITS: [u32; 3] = [1 << 10, 1 << 3, 1 << 7];
/// Bits set in each register after A5/2 loads its key and frame count.
const A5_2_FORCED_BITS: [u32; 4] = [1 << 15, 1 << 16, 1 << 18, 1 << 10];
const A5_1_MIX_CLOCKS: usize = 100;
const A5_2_MIX_CLOCKS: usize = 99;

/// One frame's cipher stream, as unpacked bits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CipherStream {
    /// The network to mobile direction.
    pub downlink: [u8; STREAM_BITS],
    /// The mobile to network direction.
    pub uplink: [u8; STREAM_BITS],
}

/// Returns a frame number's T1, T3 and T2 as the count the A5 ciphers load. A cipher
/// uses its low 22 bits, so frames past the hyperframe add bits it ignores. Bits past
/// the top of 32 are dropped.
pub const fn frame_count(frame: u32) -> u32 {
    ((frame / (26 * 51)) << 11) | ((frame % 51) << 5) | (frame % 26)
}

fn clock_register(registers: &mut [u32], index: usize) {
    let register = registers[index];
    let feedback = (register & REGISTER_TAPS[index]).count_ones() & 1;
    let mask = (1 << REGISTER_LENGTHS[index]) - 1;
    registers[index] = ((register << 1) & mask) | feedback;
}

fn clock_if_majority(registers: &mut [u32], control: [bool; 3]) {
    let majority = majority(control);
    for (index, bit) in control.into_iter().enumerate() {
        if bit == majority {
            clock_register(registers, index);
        }
    }
}

const fn majority(bits: [bool; 3]) -> bool {
    bits[0] as u8 + bits[1] as u8 + bits[2] as u8 >= 2
}

fn top_bits(registers: &[u32]) -> u32 {
    (0..3).fold(0, |bit, index| {
        bit ^ (registers[index] >> (REGISTER_LENGTHS[index] - 1))
    })
}

/// Clocks every register and XORs in each bit of the key, then of the frame count.
fn load<const N: usize>(registers: &mut [u32; N], key: &[u8; KEY_BYTES], frame: u32) {
    let key_bits = (0..u64::BITS).map(|bit| u64::from_be_bytes(*key) >> bit & 1);
    let count = u64::from(frame_count(frame));
    let count_bits = (0..FRAME_COUNT_BITS).map(|bit| count >> bit & 1);
    for bit in key_bits.chain(count_bits) {
        for index in 0..N {
            clock_register(registers, index);
            registers[index] ^= bit as u32;
        }
    }
}

/// Clocks `mix` times, then takes one output bit per clock for each direction in turn.
fn generate<const N: usize>(
    registers: &mut [u32; N],
    mix: usize,
    clock: fn(&mut [u32; N]),
    output: fn(&[u32; N]) -> u8,
) -> CipherStream {
    for _ in 0..mix {
        clock(registers);
    }
    let mut stream = CipherStream {
        downlink: [0; STREAM_BITS],
        uplink: [0; STREAM_BITS],
    };
    for bit in stream.downlink.iter_mut().chain(&mut stream.uplink) {
        clock(registers);
        *bit = output(registers);
    }
    stream
}

/// Returns the A5/0 stream, which is all zeros: no ciphering.
pub const fn a5_0() -> CipherStream {
    CipherStream {
        downlink: [0; STREAM_BITS],
        uplink: [0; STREAM_BITS],
    }
}

/// Returns a frame's A5/1 stream.
pub fn a5_1(key: &[u8; KEY_BYTES], frame: u32) -> CipherStream {
    let mut registers = [0; 3];
    load(&mut registers, key, frame);
    generate(
        &mut registers,
        A5_1_MIX_CLOCKS,
        |registers| {
            let control = [0, 1, 2].map(|index| registers[index] & A5_1_CLOCK_BITS[index] != 0);
            clock_if_majority(registers, control);
        },
        |registers| top_bits(registers) as u8,
    )
}

/// Returns a frame's A5/2 stream.
pub fn a5_2(key: &[u8; KEY_BYTES], frame: u32) -> CipherStream {
    let mut registers = [0; 4];
    load(&mut registers, key, frame);
    for (register, forced) in registers.iter_mut().zip(A5_2_FORCED_BITS) {
        *register |= forced;
    }
    generate(
        &mut registers,
        A5_2_MIX_CLOCKS,
        |registers| {
            let control = A5_2_CLOCK_BITS.map(|bit| registers[3] & bit != 0);
            clock_if_majority(registers, control);
            clock_register(registers, 3);
        },
        |registers| {
            let bit = |index: usize, position: u32| registers[index] >> position & 1 != 0;
            let nonlinear = [
                majority([bit(0, 15), !bit(0, 14), bit(0, 12)]),
                majority([!bit(1, 16), bit(1, 13), bit(1, 9)]),
                majority([bit(2, 18), bit(2, 16), !bit(2, 13)]),
            ];
            let mixed = nonlinear.into_iter().fold(false, |all, bit| all ^ bit);
            (top_bits(registers) as u8) ^ u8::from(mixed)
        },
    )
}
