# GSMTAP encoding and runtime integration

[Library overview](../README.md). Public API: [mod.rs](mod.rs); semantic input
and conversions: [message.rs](message.rs); wire encoding: [encode.rs](encode.rs).

## Implemented scope

The selected scope is both upstream constructors: `gsmtap_makemsg_ex` and its
UM-default `gsmtap_makemsg` wrapper. `GsmtapMessage` represents their inputs;
`GsmtapMessage::um(payload)` supplies UM type with zero fields for the caller to
fill, and `encode` returns owned `Vec<u8>` bytes. The original positional constructors
remain under `compat::gsmtap`.

Output is the 16-byte GSMTAP v2 base header followed by the supplied payload.
Header fields preserve the recorded byte order and integer widths, including
signed signal/SNR and high ARFCN/frame values. Antenna number and the reserved
byte are always zero because the converted constructors do not accept them.
The module publishes version/header-length/UM wire constants.

Upstream reference: `src/core/gsmtap_util.c` and
`include/osmocom/core/gsmtap_util.h` within the required ignored C checkout.
Functional placement is independent of the C src/core directory.

## Owned encoding and feature boundaries

`GsmtapMessage<'a>` borrows semantic input and payload bytes. `encode()` returns
an owned Vec with its existing allocation behavior. `encode_msgb()` allocates a
fixed-capacity `Msgb`, fills its live region directly, and copies the payload
once, without creating an intermediate Vec. Both paths use the same header
serializer. The resulting buffer owns its bytes independently of the input.
There is no separate encoded packet wrapper.

A `Msgb` holds at most 65,535 bytes, including the 16-byte GSMTAP header:
**65,519 payload bytes succeed; 65,520 return `MsgbError::CapacityTooLarge`.**
`encode()` still accepts a 65,520-byte payload and produces 65,536 bytes. The
buffer capacity limit is independent of UDP wire-size limits; successfully
encoding a large buffer does not guarantee that a socket can transmit it.
The API does not turn allocation failure into a recoverable error; it follows
ordinary Rust allocation behavior, as do the existing Vec and Msgb APIs.

| API/capability | Required features |
| --- | --- |
| `GsmtapMessage`, `um()`, `encode()`, `encode_msgb()`, constants and compatibility constructors | None |
| `GsmtapMessage::into_datagram(destination)` | `runtime` |
| Socket establishment through `net::bind_udp` | `net`, selected independently |
| GSMTAP preparation diagnostics | `runtime` and `tracing` |

Basic encoding and Msgb conversion require no Mio, socket2 or tracing. The
`runtime` feature does not enable `net` or `tracing`.

## Sending through the generic runtime

`into_datagram(destination)` encodes an owned Msgb and returns the existing
`runtime::Datagram`; it performs no I/O. `destination: None` means **no explicit
destination**. `Some(address)` carries an explicit destination. GSMTAP does not
know socket state: the generic runtime checks that no explicit destination is
supplied for a connected UDP resource, and that one is supplied for an
unconnected resource. An incompatible request returns
`Rejection::InvalidDestination` from enqueue, not from GSMTAP preparation.

Create a standard UDP socket yourself, or use [`net::bind_udp`](../net/README.md)
with the independent `net` feature. Register it with `IoRuntime::register_udp`,
then enqueue from an application callback through `RuntimeContext::try_send_udp`.
Preparation errors are `MsgbError`; enqueue failures remain
`EnqueueError<Datagram>` and return the original owned bytes and destination.
Preparation and queue acceptance do not imply send completion or peer delivery.
The runtime remains protocol-agnostic and gains no GSMTAP branches or methods.

This self-contained example uses `runtime` and a standard unconnected socket:

```rust
# #[cfg(feature = "runtime")]
# fn main() {
use osmocore_rs::{gsmtap::GsmtapMessage, runtime::*};
use std::{net::{SocketAddr, UdpSocket}, time::Duration};

struct Sender { handle: UdpHandle, completed: bool }
impl Application for Sender {
    type Command = SocketAddr;
    type Error = ();
    fn on_command(&mut self, cx: &mut RuntimeContext<'_>, destination: SocketAddr)
        -> Result<(), ()>
    {
        let datagram = GsmtapMessage::um(b"example")
            .into_datagram(Some(destination)).unwrap();
        cx.try_send_udp(self.handle, datagram).unwrap();
        Ok(())
    }
    fn on_event(&mut self, _: &mut RuntimeContext<'_>, event: RuntimeEvent<'_>)
        -> Result<(), ()>
    {
        if let RuntimeEvent::Write(outcome) = event {
            assert!(matches!(outcome.status, WriteStatus::Complete));
            self.completed = true;
        }
        Ok(())
    }
}
let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
receiver.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
let mut runtime = IoRuntime::new(RuntimeConfig::default()).unwrap();
let handle = runtime.register_udp(UdpSocket::bind("127.0.0.1:0").unwrap(),
    UdpResourceOptions::default()).unwrap();
let mut app = Sender { handle, completed: false };
runtime.control_handle().try_submit(receiver.local_addr().unwrap()).ok().unwrap();
for _ in 0..100 {
    runtime.step(&mut app, &mut DriverSet::default(), Some(Duration::from_millis(10))).unwrap();
    if app.completed { break; }
}
assert!(app.completed);
let mut bytes = [0; 64];
let length = receiver.recv(&mut bytes).unwrap();
assert_eq!(&bytes[..length], GsmtapMessage::um(b"example").encode());
# }
# #[cfg(not(feature = "runtime"))]
# fn main() {}
```

For a connected resource, use `.into_datagram(None)` and let the runtime
validate compatibility at enqueue. An empty GSMTAP payload still encodes to a
16-byte header, not an empty UDP datagram. See the [runtime guide](../runtime/README.md)
for queue budgets, write outcomes and socket lifecycle.

## Diagnostics

With `runtime` and `tracing`, successful `into_datagram()` calls emit one **trace**
event at `osmocore_rs::gsmtap`: `operation = "prepare_datagram"`, `message_type`,
`arfcn`, `timeslot`, `channel_type`, `frame_number`, `payload_len`, and
`explicit_destination` (a boolean). This high-frequency event records preparation,
not enqueue acceptance or successful transmission, and inherits the current
application span. Plain encoding and failed preparation emit no GSMTAP event.
No payload bytes, packet bytes, endpoint addresses or pointers are logged.
The library installs no subscriber. See the [diagnostics contract](../diagnostics/README.md)
for application-owned subscribers and filters; enable `osmocore_rs::gsmtap=trace`
to capture preparation. Runtime/network diagnostics retain their own targets.

## Testing and verification

`tests/gsmtap/main.rs` registers `gsmtap_reference_vectors.rs`, which checks the
committed packet observations byte-for-byte. The seven JSON cases cover the
base header, SIM ATR, ABIS signed boundaries, UM empty payload, uplink, PCS and
combined boundary fields, plus UM-wrapper equivalence. Every case checks both
Vec and Msgb encoding; source unit tests retain boundary
header bytes, owned payload output and UM-wrapper checks. Public tests also
check the exact Msgb ceiling, independent ownership, connected/unconnected UDP
transmission, runtime destination validation and rejection ownership. Isolated
`gsmtap_diagnostics` assertions cover structured trace fields, application spans,
filtering and payload/endpoint privacy. All packet observations are C
outputs; the sibling utility validates their structure/provenance and supports
packet inspection without acting as the behavior oracle.

Older evidence records reference revision
950430e829a3dc1d162aa241bc0505745c5a7311. Generation must use the local checkout's
headers and linked/runtime library. Ordinary Rust tests read committed packets
and need no C installation or sibling repository. The [GSMTAP architecture](https://github.com/Silent-Wolf-Labs/osmocore-rs/blob/main/docs/README.md#gsmtap-encoding-and-runtime-delivery) describes encoding
and generic runtime composition.

## Remaining work

A general GSMTAP parser/decoder, malformed-packet behavior, protocol-specific
sinks and additional upstream helpers remain deferred. Socket lifecycle, sending
and work queues are provided by the existing generic runtime, rather than C
GSMTAP source/global-state mechanics.
The upstream inventory includes 17 gsmtap_util exported functions; covering two
constructors does not imply coverage of that entire family. Additional protocols
or header-control fields require separate scope and evidence.

## Fundamental Rust/C differences

Payload is borrowed as a slice while constructing input; encoded output owns its
Vec or Msgb bytes. C allocation contexts, message pointers and freeing are
replaced by Rust ownership. No C struct-layout/ABI or C transport/global-state
parity is promised. `um` is a Rust convenience, while the compatibility wrapper
retains the original positional field/default policy. No parser validation or
new malformed-input policy is inferred from successful constructor evidence.
