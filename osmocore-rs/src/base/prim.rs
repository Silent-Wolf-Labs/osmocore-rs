// SPDX-License-Identifier: GPL-3.0-or-later

//! Inter-layer primitive metadata and event mapping.
//!
//! A header borrows its optional message exclusively; it is not overlaid onto
//! message bytes and does not own or free the message. This avoids C's embedded
//! header/back-pointer cycle. Raw numeric values remain representable.

use std::borrow::Cow;

use super::msgb::Msgb;

/// Raw operation bits, matching the fixture's 32-bit C enum representation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrimOperation(pub u32);

pub const PRIM_OP_REQUEST: PrimOperation = PrimOperation(0);
pub const PRIM_OP_RESPONSE: PrimOperation = PrimOperation(1);
pub const PRIM_OP_INDICATION: PrimOperation = PrimOperation(2);
pub const PRIM_OP_CONFIRM: PrimOperation = PrimOperation(3);

/// Bounded counterpart of `osmo_prim_op_names`, without C's null terminator.
pub const OPERATION_NAMES: [(PrimOperation, &str); 4] = [
    (PRIM_OP_REQUEST, "request"),
    (PRIM_OP_RESPONSE, "response"),
    (PRIM_OP_INDICATION, "indication"),
    (PRIM_OP_CONFIRM, "confirm"),
];

pub const SAP_GSM_SHIFT: u32 = 24;
pub const SAP_GSM_BASE: u32 = 0x01 << SAP_GSM_SHIFT;
pub const SAP_TETRA_BASE: u32 = 0x02 << SAP_GSM_SHIFT;
pub const SAP_SS7_BASE: u32 = 0x03 << SAP_GSM_SHIFT;
pub(crate) const OSMO_NO_EVENT: u32 = u32::MAX;

/// Safe counterpart of the C primitive header; `None` represents a null message.
///
/// The lifetime prevents dangling association and mutable aliasing. Drop only
/// releases the borrow. No C structure layout or serialization is implied.
#[derive(Debug, Default)]
pub struct PrimHeader<'msg> {
    pub sap: u32,
    pub primitive: u32,
    pub operation: PrimOperation,
    pub msg: Option<&'msg mut Msgb>,
}

impl<'msg> PrimHeader<'msg> {
    pub fn new(
        sap: u32,
        primitive: u32,
        operation: PrimOperation,
        msg: Option<&'msg mut Msgb>,
    ) -> Self {
        Self {
            sap,
            primitive,
            operation,
            msg,
        }
    }
}

/// `OSMO_PRIM` evaluated with an unsigned 32-bit primitive operand.
/// Higher primitive bits are discarded; only the low operation byte is used.
pub const fn packed_id(primitive: u32, operation: PrimOperation) -> u32 {
    (primitive << 8) | (operation.0 & 0xff)
}

/// Exact C operation-name text, with stable storage instead of a reused buffer.
pub fn operation_name(operation: PrimOperation) -> Cow<'static, str> {
    OPERATION_NAMES
        .iter()
        .find(|(value, _)| *value == operation)
        .map(|(_, name)| Cow::Borrowed(*name))
        .unwrap_or_else(|| Cow::Owned(format!("unknown 0x{:x}", operation.0)))
}

/// Typed-context counterpart of `osmo_prim_cb`, without raw pointers or FFI.
/// A callback may edit the header/message, but cannot free the borrowed message.
pub type PrimCallback<Context> = for<'msg> fn(&mut PrimHeader<'msg>, &mut Context) -> i32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimEventMap {
    pub sap: u32,
    pub primitive: u32,
    pub operation: PrimOperation,
    pub event: u32,
}

#[cfg(test)]
mod tests {
    use super::legacy::*;
    use super::*;

    #[test]
    fn bounded_maps_and_stable_names() {
        let header = PrimHeader::default();
        assert_eq!(osmo_event_for_prim(&header, &[]), OSMO_NO_EVENT);
        let map = PrimEventMap {
            sap: 1,
            primitive: 0,
            operation: PRIM_OP_REQUEST,
            event: 5,
        };
        assert_eq!(osmo_event_for_prim(&header, &[map]), OSMO_NO_EVENT);
        assert_eq!(
            osmo_event_for_prim(&header, &[PrimEventMap { sap: 0, ..map }]),
            5
        );
        let first = operation_name(PrimOperation(4));
        let second = operation_name(PrimOperation(u32::MAX));
        assert_eq!(first, "unknown 0x4");
        assert_eq!(second, "unknown 0xffffffff");
    }
}

pub(crate) mod legacy {
    use super::*;
    /// Replace all header fields without changing the associated message.
    pub fn osmo_prim_init<'msg>(
        header: &mut PrimHeader<'msg>,
        sap: u32,
        primitive: u32,
        operation: PrimOperation,
        msg: Option<&'msg mut Msgb>,
    ) {
        *header = PrimHeader::new(sap, primitive, operation, msg);
    }

    /// Return the first exact match before a sentinel, or `OSMO_NO_EVENT`.
    /// Slice exhaustion safely ends a map even if C's required sentinel is absent.
    pub fn osmo_event_for_prim(header: &PrimHeader<'_>, maps: &[PrimEventMap]) -> u32 {
        for map in maps {
            if map.event == OSMO_NO_EVENT {
                break;
            }
            if map.sap == header.sap
                && map.primitive == header.primitive
                && map.operation == header.operation
            {
                return map.event;
            }
        }
        OSMO_NO_EVENT
    }
}

impl PrimHeader<'_> {
    /// Pack this header's primitive and operation into its 32-bit identifier.
    pub fn packed_id(&self) -> u32 {
        packed_id(self.primitive, self.operation)
    }
}
/// Find the first matching entry in an ordinary slice, without a C terminator.
pub fn event_for_primitive(header: &PrimHeader<'_>, maps: &[PrimEventMap]) -> Option<u32> {
    maps.iter()
        .find(|map| {
            map.sap == header.sap
                && map.primitive == header.primitive
                && map.operation == header.operation
        })
        .map(|map| map.event)
}
