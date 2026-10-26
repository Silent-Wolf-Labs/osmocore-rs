# Networking establishment

Enable the optional Cargo feature `net` for reusable IPv4/IPv6 TCP/UDP setup.
The default feature set stays empty. `std::net` supplies endpoint types, parsing,
resolution and the returned owned sockets; `socket2` supplies pre-bind options,
local binding before connect and a bounded TCP connection attempt. No runtime,
readiness registry, worker thread, retry loop or shutdown policy is installed.

```rust,no_run
# #[cfg(feature = "net")]
# fn main() -> Result<(), Box<dyn std::error::Error>> {
use osmocore_rs::net::{listen_tcp, connect_tcp, bind_udp, SocketOptions, TcpOptions};
use std::{net::SocketAddr, time::Duration};

let local: SocketAddr = "127.0.0.1:0".parse()?;
let listener = listen_tcp(local, 16, &TcpOptions::default())?;
let stream = connect_tcp(Some(local), listener.local_addr()?,
    Duration::from_secs(2), &TcpOptions::default())?;
let (accepted, _) = listener.accept()?;
let datagram = bind_udp(local, None, &SocketOptions::default())?;
// The application owns native sockets and their I/O/readiness/shutdown policy.
drop((stream, accepted, listener, datagram));
# Ok(())
# }
# #[cfg(not(feature = "net"))]
# fn main() {}
```

## Establishment contract

- `resolve(host, port, AddressFamily)` explicitly invokes the synchronous OS
  resolver, preserves its order, filters IPv4/IPv6 and removes duplicates.
  Empty results after filtering return `Operation::Resolve`/`AddrNotAvailable`.
  Resolution does not choose an endpoint, bind, connect or retry. There is no
  library DNS timeout; callers needing bounded/asynchronous resolution own it.
- `listen_tcp(local, backlog, options)` configures, binds and listens on one
  numeric endpoint. Positive backlog must fit `i32`; the OS may cap it. Port zero
  requests an ephemeral port; inspect `local_addr()` on the returned listener.
- `connect_tcp(local, remote, timeout, options)` optionally binds a specific local
  address/source port, then waits for one TCP connection attempt with a required
  nonzero timeout. Address families must match; no implicit resolution, candidate
  fallback or retries occur. The timeout covers connection establishment, not
  socket configuration, DNS, application reads/writes or all candidate attempts.
- `bind_udp(local, peer, options)` binds one numeric endpoint and optionally
  selects a default peer. UDP connect is not a handshake and does not establish
  peer reachability. Standard `send_to`/`recv_from` or `send`/`recv` remain available.

Every successful call transfers an owned standard-library socket to the caller;
all partially established sockets close through RAII on failure. No raw descriptor
ownership is transferred or lost. Socket handles/addresses, including IPv6 scope
IDs, use standard Rust types. There is no fixed local port, wildcard binding,
address-family preference or process-global networking state hidden in the API.
Applications explicitly choose loopback versus wildcard exposure and candidate
retry policy. Dropping the returned socket provides normal OS resource cleanup;
application connection teardown/shutdown policy remains external.

`SocketOptions` selects pre-bind `SO_REUSEADDR`, optional `IPV6_V6ONLY`, optional
six-bit DSCP and the mode of the returned socket. Defaults are reuse disabled,
DSCP unchanged, OS-default IPv6-only setting and blocking returned sockets.
An explicit IPv6-only option on an IPv4 endpoint, mapped IPv4 addresses with
IPv6-only mode enabled, mismatched local/peer families, DSCP above 63, zero
connect timeout and invalid backlog are rejected before socket creation.
Setting DSCP preserves ECN bits and supports explicitly setting DSCP zero.
Unsupported DSCP platforms/families return `Unsupported` instead of silently
ignoring the setting. The socket2 IPv4 TOS API is unavailable on Fuchsia, Redox,
Solaris, Haiku and WASI; IPv6 traffic-class setup is available on its supported
Unix targets (including Linux/macOS/BSD), and is explicitly unsupported by this
helper on Windows. OS policy may still reject otherwise supported settings.

`TcpOptions` adds TCP_NODELAY and SO_KEEPALIVE, both default false. Options are
applied to the new stream/listener before bind/connect. Inheritance by accepted
streams (including blocking mode) is OS-defined; applications configure accepted
streams explicitly when they need a specific policy.

`nonblocking` describes the returned socket, **not** asynchronous establishment.
TCP connect still waits for success/failure/timeout before returning. This slice
does not return pending TCP connections, poll readiness, register descriptors,
spawn workers, own accept loops or bridge timers/FSMs into an I/O runtime.
Applications can hand standard sockets to their chosen runtime afterward.

## Errors and optional diagnostics

`NetError` retains `Operation` and the original `std::io::Error`, including its
kind, raw OS code and error source. `io_error()` borrows it; `into_io_error()`
returns ownership. Phases distinguish `Validate`, `Resolve`, `Create`, `Configure`,
`Bind`, `Listen`, `Connect` and `SetNonblocking`. Validation uses `InvalidInput`;
OS errors retain their original kind/code. Configuration failure closes the
socket and is returned. There is no collapsed C integer errno compatibility API.

With both `net` and `tracing` enabled, target `osmocore_rs::net` emits:

| Level | Span/event | Fields |
| --- | --- | --- |
| trace | `net_establishment` span | `operation`: `resolve`, `tcp_listen`, `tcp_connect`, `udp_bind` |
| debug | completed socket establishment | `operation`, `family` (`ipv4`/`ipv6`), `nonblocking` |
| debug | completed resolution | `operation = "resolve"`, `address_count` |
| warn | returned failure | `operation`, `phase`, `error_category`, optional numeric `os_error` |

Categories are `invalid_input`, `address_in_use`, `address_not_available`,
`connection_refused`, `timeout`, `permission_denied`, `unsupported`, and `io` for
other kinds. No hostnames, addresses, ports, payloads or formatted error messages
are logged. Spans inherit active application context. No subscriber is installed
and the application owns output, formatting and filters. See the existing
[diagnostics guide](../diagnostics/README.md) for application configuration; add a filter
such as `osmocore_rs::net=debug`. Turning tracing off leaves network behavior and
error semantics unchanged and removes all networking instrumentation.

## Reference capability mapping and validation

Relevant local C reference: `src/core/socket.c`, `include/osmocom/core/socket.h`
and `tests/socket/socket_test.c` at verified upstream revision
`950430e829a3dc1d162aa241bc0505745c5a7311`.

| Upstream useful capability | Rust-native implementation |
| --- | --- |
| Address handling and getaddrinfo candidates | `std::net::SocketAddr` parsing/formatting and explicit `resolve` |
| Bind and connect with a selected source address/port | `connect_tcp(Some(local), ...)` and connected `bind_udp` |
| TCP bind/listen and UDP bind | Owned standard-library listener/datagram sockets |
| Pre-bind reuse/address-family settings | `SocketOptions` through socket2 |
| Nonblocking configuration | Explicit mode on returned sockets |
| DSCP configuration | Checked 0..=63, family-aware option, preserved ECN |
| Descriptor cleanup on failure | Owned socket RAII with original structured I/O errors |

This is capability parity for the bounded subset, not C API, flag, formatting,
errno, descriptor-number, address-order or automatic-family-selection parity.
C commonly enables reuse for TCP automatically; Rust requires explicit opt-in.
C setup can treat DSCP failures as nonfatal; Rust returns them. C can return an
in-progress nonblocking connection; this subset returns only completed TCP
connections. SCTP, multicast membership/setup, broadcast policy, Unix-domain/raw
sockets, priority/reuseport tuning, multiaddress retries and specialized protocol
options remain deferred. Standard returned sockets still expose their ordinary
broadcast/multicast methods; this layer does not add implicit behavior around them.

The C harness lives in the sibling `gsmtap-rs/reference/network/`. It builds and
links only the canonical local reference. Its six normalized IPv4 loopback cases
record bound/peer identity predicates, mode, reuse, DSCP, invalid DSCP rejection
and TCP/UDP payload delivery. Ephemeral port values are intentionally not frozen.
Source and generator provenance accompany the new fixtures. Shared fixtures and
reader are outside packet/replay loaders. Existing observations are unchanged.

Repository `net` tests compare these observations and independently check real
IPv4/IPv6 loopback traffic, configured native options, local source selection,
invalid inputs, nonblocking results, connection-refusal errors and cleanup after
failed connection. IPv6 tests skip only genuine unavailable-family/address
conditions, not permission errors. Linux is exercised here; other OS behavior is
not certified. Scoped-subscriber assertions run in the separate `net_diagnostics`
target to avoid concurrent first-callsite registration by uninstrumented tests.
No external services, fixed listening ports, sleeps or public DNS are needed.

`socket2` 0.6.5 is optional behind `net`, with defaults disabled and its `all`
feature enabled to expose Unix IPv6 traffic-class methods. Its declared compiler
minimum is Rust 1.70, below this project's current stable edition-2024 toolchain.
The normal Linux dependency graph adds only socket2 and libc; Windows additionally
selects windows-sys/windows-link. No async runtime or reactor dependency is added.

The independent `runtime` feature provides I/O/lifecycle composition for
established sockets.

The `net_diagnostics` Cargo target is registered explicitly at
`tests/net/diagnostics/main.rs`; it stays in a separate process from the
loopback tests in `tests/net/main.rs`.
