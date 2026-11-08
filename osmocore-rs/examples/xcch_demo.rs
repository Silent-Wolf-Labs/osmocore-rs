// SPDX-License-Identifier: GPL-3.0-or-later
//! Sends a BCCH System Information block through a noisy channel and out as GSMTAP.
//!
//! Each block is built in a `Msgb`, encoded into four bursts with GSM 05.03 xCCH
//! coding, given bit errors chosen by a PRBS-15 generator, then decoded. Blocks that
//! pass the parity check are sent over UDP as GSMTAP, so the gsmtap-rs workbench or
//! Wireshark shows them live. Blocks that fail are reported and not sent, as a receiver
//! would drop them.
//!
//! Run with `cargo run --example xcch_demo -- [--errors PERCENT] [--blocks N] [--to ADDRESS]`.
//! Without `--errors`, the error rate climbs from 0% to 12% over the run, which shows
//! where decoding stops working. `--to` defaults to `127.0.0.1:4729`, where the
//! workbench listens.
use osmocore_rs::{
    base::bits::to_soft,
    base::gsmtap::{GSMTAP_UDP_PORT, GsmtapMessage, channel},
    base::msgb::Msgb,
    base::prbs::{PRBS15, PrbsState},
    coding::gsm0503::{
        XCCH_BURST_BITS, XCCH_BURSTS_BITS, XCCH_CODED_BITS, XCCH_L2_BYTES,
        XCCH_STEALING_FLAG_OFFSETS, xcch_decode, xcch_encode,
    },
    gsm::gsm_utils::GsmTime,
    gsm::gsm0502::{TDMA_FRAME_DURATION_US, frame_add},
};
use std::{
    env,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    process::ExitCode,
    thread,
    time::Duration,
};

/// A System Information Type 3 body from libosmocore's BSSGP RIM test, starting at the
/// message type. Its rest octets fill the block, so no padding is needed.
const SI3_BODY: [u8; 21] = [
    0x1b, 0x75, 0x30, 0x00, 0xf1, 0x10, 0x23, 0x6e, 0xc9, 0x03, 0x3c, 0x27, 0x47, 0x40, 0x79, 0x00,
    0x00, 0x3c, 0x0b, 0x2b, 0x2b,
];
/// The L2 pseudo length: 18 octets before the rest octets, with the low bits `01`.
const SI3_PSEUDO_LENGTH: u8 = (18 << 2) | 0b01;
/// Radio resource management, the protocol System Information belongs to.
const PROTOCOL_RR: u8 = 0x06;
/// The BCCH block starts in frame 2 of each 51-frame multiframe.
const BCCH_FIRST_FRAME: u32 = 2;
const MULTIFRAME: u32 = 51;
const ARFCN: u16 = 20;
const DEFAULT_BLOCKS: u32 = 25;
const RAMP_TOP_PERCENT: f64 = 12.0;

struct Options {
    fixed_percent: Option<f64>,
    blocks: u32,
    destination: String,
}

fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        fixed_percent: None,
        blocks: DEFAULT_BLOCKS,
        destination: format!("127.0.0.1:{GSMTAP_UDP_PORT}"),
    };
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--errors" => match value.parse::<f64>() {
                Ok(percent) if (0.0..=100.0).contains(&percent) => {
                    options.fixed_percent = Some(percent)
                }
                _ => return Err(format!("--errors must be 0 to 100, got {value}")),
            },
            "--blocks" => {
                options.blocks = value
                    .parse()
                    .map_err(|_| format!("--blocks must be a whole number, got {value}"))?
            }
            "--to" => options.destination = value,
            _ => return Err(format!("unknown option {flag}")),
        }
    }
    Ok(options)
}

/// Builds the L2 frame the way a stack would: the body first, then the headers pushed
/// in front of it.
fn build_si3() -> [u8; XCCH_L2_BYTES] {
    let mut message = Msgb::with_headroom(XCCH_L2_BYTES, 2).expect("fits a Msgb");
    message.put_slice(&SI3_BODY).expect("room for the body");
    message
        .push_slice(&[SI3_PSEUDO_LENGTH, PROTOCOL_RR])
        .expect("headroom");
    message.data().try_into().expect("23 bytes")
}

/// Turns coded bits into soft bits, flipping each data bit with the given probability.
/// A PRBS-15 generator supplies the random numbers, 10 bits at a time, so every run
/// gives the same errors. The stealing flags are left alone, since the decoder ignores
/// them.
fn noisy_channel(
    bits: &[u8; XCCH_BURSTS_BITS],
    percent: f64,
    prbs: &mut PrbsState,
) -> ([i8; XCCH_BURSTS_BITS], usize) {
    let threshold = (percent / 100.0 * 1024.0).round() as u64;
    let mut flipped = 0;
    let mut soft = [0; XCCH_BURSTS_BITS];
    for (index, (&bit, soft)) in bits.iter().zip(&mut soft).enumerate() {
        let random = prbs.next_bits(10);
        let clean = to_soft(bit);
        let stealing_flag = XCCH_STEALING_FLAG_OFFSETS.contains(&(index % XCCH_BURST_BITS));
        *soft = if random < threshold && !stealing_flag {
            flipped += 1;
            -clean
        } else {
            clean
        };
    }
    (soft, flipped)
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            eprintln!("usage: xcch_demo [--errors PERCENT] [--blocks N] [--to ADDRESS]");
            return ExitCode::FAILURE;
        }
    };
    let destination: SocketAddr = match options
        .destination
        .to_socket_addrs()
        .map(|mut addresses| addresses.next())
    {
        Ok(Some(address)) => address,
        Ok(None) | Err(_) => {
            eprintln!("can't resolve {}", options.destination);
            return ExitCode::FAILURE;
        }
    };
    // An unconnected socket, so a missing listener doesn't turn sends into errors.
    let local = if destination.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = match UdpSocket::bind(local) {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("can't open a UDP socket: {error}");
            return ExitCode::FAILURE;
        }
    };

    let frame = build_si3();
    let coded = xcch_encode(&frame);
    let mut prbs = PrbsState::new(PRBS15);
    let mut frame_number = BCCH_FIRST_FRAME;
    let mut sent = 0;
    println!("SI3 frame: {}", hex(&frame));
    println!("Sending BCCH on ARFCN {ARFCN} to {destination}\n");

    for block in 0..options.blocks {
        let percent = options.fixed_percent.unwrap_or_else(|| {
            RAMP_TOP_PERCENT * f64::from(block) / f64::from(options.blocks.saturating_sub(1).max(1))
        });
        let (soft, flipped) = noisy_channel(&coded, percent, &mut prbs);
        let decoded = xcch_decode(&soft);
        let time = GsmTime::from_frame(frame_number).expect("below the hyperframe");
        print!(
            "block {block:2}  fn {time}  flipped {flipped:3}/{XCCH_CODED_BITS} ({percent:4.1}%)  "
        );
        match decoded.data {
            Some(data) if data == frame => {
                let packet = GsmtapMessage {
                    arfcn: ARFCN,
                    channel_type: channel::BCCH,
                    frame_number,
                    signal_dbm: -70,
                    ..GsmtapMessage::um(&data)
                }
                .encode();
                match socket.send_to(&packet, destination) {
                    Ok(_) => {
                        sent += 1;
                        println!(
                            "decoded, {} bit errors corrected, sent {} bytes",
                            decoded.bit_errors,
                            packet.len()
                        );
                    }
                    Err(error) => println!("decoded, but sending failed: {error}"),
                }
            }
            // The 40-bit parity check makes a wrong frame that passes it very unlikely.
            Some(_) => println!("decoded to the wrong frame, not sent"),
            None => println!(
                "parity check failed ({} bit errors), not sent",
                decoded.bit_errors
            ),
        }
        frame_number = frame_add(frame_number, MULTIFRAME);
        thread::sleep(Duration::from_micros(u64::from(
            MULTIFRAME * TDMA_FRAME_DURATION_US,
        )));
    }
    println!("\n{sent} of {} blocks sent", options.blocks);
    ExitCode::SUCCESS
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
