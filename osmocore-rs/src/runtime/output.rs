// SPDX-License-Identifier: GPL-3.0-or-later
use super::resource::Output;
use std::{io, net::SocketAddr};
pub(crate) enum Attempt {
    Progress(usize),
    Complete(usize),
    Blocked,
    Interrupted,
    Failed(io::Error),
}
/// One transport operation. The closure is the private transport boundary used
/// by both Mio sources and scripted short-write tests.
pub(crate) fn attempt(
    output: &mut Output,
    tcp: bool,
    budget: usize,
    send: impl FnOnce(&[u8], Option<SocketAddr>) -> io::Result<usize>,
) -> Attempt {
    let data = output.message.data();
    if tcp && output.offset == data.len() {
        return Attempt::Complete(0);
    }
    let remaining = &data[output.offset..];
    let bytes = if tcp {
        &remaining[..remaining.len().min(budget)]
    } else {
        remaining
    };
    match send(bytes, output.destination) {
        Ok(n) if n > bytes.len() => {
            Attempt::Failed(io::Error::other("transport exceeded supplied length"))
        }
        Ok(0) if tcp => Attempt::Failed(io::Error::new(
            io::ErrorKind::WriteZero,
            "nonempty TCP write returned zero",
        )),
        Ok(n) => {
            output.offset += n;
            if !tcp && n != bytes.len() {
                Attempt::Failed(io::Error::other("short datagram send"))
            } else if output.offset == data.len() {
                Attempt::Complete(n)
            } else {
                Attempt::Progress(n)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => Attempt::Blocked,
        Err(e) if e.kind() == io::ErrorKind::Interrupted => Attempt::Interrupted,
        Err(e) => Attempt::Failed(e),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{msgb::Msgb, runtime::WriteId};
    fn entry(bytes: &[u8]) -> Output {
        let mut message = Msgb::new(64).unwrap();
        message.put(bytes.len()).unwrap();
        message.data_mut().copy_from_slice(bytes);
        Output {
            message,
            destination: None,
            offset: 0,
            id: WriteId(1),
            context: Default::default(),
        }
    }
    #[test]
    fn scripted_partial_block_interrupt_and_fifo() {
        let mut wire = Vec::new();
        for text in [b"abcdef".as_slice(), b"ghij"] {
            let mut output = entry(text);
            for outcome in [
                Ok(2),
                Err(io::ErrorKind::WouldBlock),
                Err(io::ErrorKind::Interrupted),
                Ok(1),
                Ok(64),
            ] {
                let result = attempt(&mut output, true, 3, |data, _| match outcome {
                    Ok(n) => {
                        let n = n.min(data.len());
                        wire.extend_from_slice(&data[..n]);
                        Ok(n)
                    }
                    Err(kind) => Err(kind.into()),
                });
                if matches!(result, Attempt::Complete(_)) {
                    break;
                }
            }
            assert_eq!(output.offset, text.len());
            assert_eq!(output.message.capacity(), 64);
        }
        assert_eq!(wire, b"abcdefghij");
    }
    #[test]
    fn empty_tcp_skips_io_but_empty_udp_sends() {
        let mut output = entry(b"");
        assert!(matches!(
            attempt(&mut output, true, 10, |_, _| panic!("empty TCP IO")),
            Attempt::Complete(0)
        ));
        let mut sent = false;
        assert!(matches!(
            attempt(&mut output, false, 10, |bytes, _| {
                sent = true;
                assert!(bytes.is_empty());
                Ok(0)
            }),
            Attempt::Complete(0)
        ));
        assert!(sent);
    }
    #[test]
    fn zero_tcp_and_short_udp_fail_with_progress() {
        let mut output = entry(b"abc");
        assert!(
            matches!(attempt(&mut output, true, 10, |_, _| Ok(0)), Attempt::Failed(e) if e.kind() == io::ErrorKind::WriteZero)
        );
        assert!(matches!(
            attempt(&mut output, false, 10, |_, _| Ok(2)),
            Attempt::Failed(_)
        ));
        assert_eq!(output.offset, 2);
    }
}

#[cfg(test)]
mod datagram_tests {
    use super::*;
    #[test]
    fn blocked_and_failed_datagrams_keep_packet_identity_and_destination() {
        let mut message = crate::msgb::Msgb::new(32).unwrap();
        message.put(3).unwrap();
        message.data_mut().copy_from_slice(b"abc");
        let destination = Some("127.0.0.1:9".parse().unwrap());
        let mut output = Output {
            message,
            destination,
            offset: 0,
            id: super::super::WriteId(1),
            context: Default::default(),
        };
        assert!(matches!(
            attempt(&mut output, false, 1, |bytes, peer| {
                assert_eq!(bytes, b"abc");
                assert_eq!(peer, destination);
                Err(io::ErrorKind::WouldBlock.into())
            }),
            Attempt::Blocked
        ));
        assert_eq!(output.offset, 0);
        assert!(matches!(
            attempt(&mut output, false, 1, |bytes, peer| {
                assert_eq!(bytes, b"abc");
                assert_eq!(peer, destination);
                Ok(3)
            }),
            Attempt::Complete(3)
        ));
    }
}
