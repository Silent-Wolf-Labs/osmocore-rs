// SPDX-License-Identifier: GPL-3.0-or-later

//! Core TDef parity against the recorded 64-bit C ABI (unsigned long/long = 64,
//! int = 32). Values use `u64`; 32-bit ULONG_MAX behavior is outside this claim.
//! FSM state transitions integrate through the owned `fsm` runtime.
//! VTY remains outside this module.
//!
//! Slice adapters validate termination before operating and never scan outside
//! supplied storage. `None` represents a null C table; `Some(&[])` is an
//! unterminated table. A sentinel ignores current value, unit and bounds.

use std::{borrow::Cow, fmt};

/// Numeric C unit, including unknown enum values.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TDefUnit(pub i32);
impl TDefUnit {
    pub const S: Self = Self(0);
    pub const MS: Self = Self(1);
    pub const M: Self = Self(2);
    pub const CUSTOM: Self = Self(3);
    pub const US: Self = Self(4);

    fn micros(self) -> Option<u64> {
        match self {
            Self::S => Some(1_000_000),
            Self::MS => Some(1_000),
            Self::M => Some(60_000_000),
            Self::US => Some(1),
            _ => None,
        }
    }
}

/// Public C name table, including its null-name terminator.
pub(crate) const OSMO_TDEF_UNIT_NAMES: [(TDefUnit, Option<&str>); 6] = [
    (TDefUnit::S, Some("s")),
    (TDefUnit::MS, Some("ms")),
    (TDefUnit::M, Some("m")),
    (TDefUnit::CUSTOM, Some("custom-unit")),
    (TDefUnit::US, Some("us")),
    (TDefUnit::S, None),
];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TDef<'a> {
    pub id: i32,
    pub default_val: u64,
    pub unit: TDefUnit,
    pub desc: Option<&'a str>,
    pub val: u64,
    pub min_val: u64,
    pub max_val: u64,
}
impl TDef<'_> {
    pub fn is_sentinel(&self) -> bool {
        self.id == 0 && self.default_val == 0 && self.desc.is_none()
    }
}

/// Assertion/undefined-behavior equivalents are errors, never process aborts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TDefError {
    MissingTerminator,
    MissingGroupTerminator,
    MissingDefinition {
        id: i32,
    },
    NegativeFallback {
        id: i32,
        fallback: i64,
    },
    InvalidConversion {
        from: TDefUnit,
        to: TDefUnit,
    },
    RejectedValue {
        id: i32,
        value: u64,
        min: u64,
        max: u64,
    },
    InvalidDefault {
        index: usize,
        id: i32,
        value: u64,
        min: u64,
        max: u64,
    },
    InvalidState {
        state: u32,
    },
}
impl TDefError {
    /// Recorded C status where C returns normally. Assertion/UB equivalents and
    /// bounded-adapter errors have no C integer status.
    pub(crate) fn c_status(&self) -> Option<i32> {
        match self {
            Self::MissingDefinition { .. } => Some(-17), // -EEXIST
            Self::RejectedValue { .. } => Some(-34),     // -ERANGE
            _ => None,
        }
    }
}
impl fmt::Display for TDefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TDefError {}

pub(crate) fn convert(value: u64, from: TDefUnit, to: TDefUnit) -> Result<u64, TDefError> {
    if value == 0 || from == to || from == TDefUnit::CUSTOM || to == TDefUnit::CUSTOM {
        return Ok(value);
    }
    let (source, target) = match (from.micros(), to.micros()) {
        (Some(source), Some(target)) => (source, target),
        _ => return Err(TDefError::InvalidConversion { from, to }),
    };
    if source > target {
        Ok(value.saturating_mul(source / target))
    } else {
        let factor = target / source;
        Ok(value / factor + u64::from(!value.is_multiple_of(factor)))
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StateTimeout {
    pub id: i32,
    pub keep_timer: bool,
}

#[derive(Debug, Default)]
pub struct TDefGroup<'t, 'd> {
    pub name: Option<&'d str>,
    pub desc: Option<&'d str>,
    pub tdefs: Option<&'t [TDef<'d>]>,
}

pub(crate) mod legacy {
    use super::*;
    pub fn osmo_tdef_unit_name(unit: TDefUnit) -> Cow<'static, str> {
        OSMO_TDEF_UNIT_NAMES
            .iter()
            .find_map(|(u, name)| if *u == unit { *name } else { None })
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(format!("unknown 0x{:x}", unit.0 as u32)))
    }

    /// Bounded C-sentinel adapter. The returned prefix excludes the sentinel.
    pub fn osmo_tdef_for_each<'t, 'd>(
        table: Option<&'t [TDef<'d>]>,
    ) -> Result<&'t [TDef<'d>], TDefError> {
        match table {
            None => Ok(&[]),
            Some(table) => {
                let end = table
                    .iter()
                    .position(TDef::is_sentinel)
                    .ok_or(TDefError::MissingTerminator)?;
                Ok(&table[..end])
            }
        }
    }

    pub fn osmo_tdef_for_each_mut<'t, 'd>(
        table: Option<&'t mut [TDef<'d>]>,
    ) -> Result<&'t mut [TDef<'d>], TDefError> {
        match table {
            None => Ok(&mut []),
            Some(table) => {
                let end = table
                    .iter()
                    .position(TDef::is_sentinel)
                    .ok_or(TDefError::MissingTerminator)?;
                Ok(&mut table[..end])
            }
        }
    }

    pub fn osmo_tdef_get_entry<'t, 'd>(
        table: Option<&'t [TDef<'d>]>,
        id: i32,
    ) -> Result<Option<&'t TDef<'d>>, TDefError> {
        Ok(osmo_tdef_for_each(table)?.iter().find(|t| t.id == id))
    }

    pub fn osmo_tdef_get_entry_mut<'t, 'd>(
        table: Option<&'t mut [TDef<'d>]>,
        id: i32,
    ) -> Result<Option<&'t mut TDef<'d>>, TDefError> {
        Ok(osmo_tdef_for_each_mut(table)?
            .iter_mut()
            .find(|t| t.id == id))
    }

    pub fn osmo_tdef_get(
        table: Option<&[TDef<'_>]>,
        id: i32,
        as_unit: TDefUnit,
        fallback: i64,
    ) -> Result<u64, TDefError> {
        match osmo_tdef_get_entry(table, id)? {
            Some(t) => convert(t.val, t.unit, as_unit),
            None if fallback >= 0 => Ok(fallback as u64),
            None => Err(TDefError::NegativeFallback { id, fallback }),
        }
    }

    pub fn osmo_tdef_val_in_range(t: &TDef<'_>, value: u64) -> bool {
        value >= t.min_val && (t.max_val == 0 || value <= t.max_val)
    }

    pub fn osmo_tdef_set(
        table: Option<&mut [TDef<'_>]>,
        id: i32,
        value: u64,
        unit: TDefUnit,
    ) -> Result<(), TDefError> {
        let t = osmo_tdef_get_entry_mut(table, id)?.ok_or(TDefError::MissingDefinition { id })?;
        let value = convert(value, unit, t.unit)?;
        if !osmo_tdef_val_in_range(t, value) {
            return Err(TDefError::RejectedValue {
                id,
                value,
                min: t.min_val,
                max: t.max_val,
            });
        }
        t.val = value;
        Ok(())
    }

    /// Compatibility wrapper: 0, -EEXIST or -ERANGE for ordinary C results;
    /// assertion/UB equivalents remain typed errors.
    pub fn osmo_tdef_set_status(
        table: Option<&mut [TDef<'_>]>,
        id: i32,
        value: u64,
        unit: TDefUnit,
    ) -> Result<i32, TDefError> {
        match osmo_tdef_set(table, id, value, unit) {
            Ok(()) => Ok(0),
            Err(error) => match error.c_status() {
                Some(status) => Ok(status),
                None => Err(error),
            },
        }
    }

    /// Reset in order. On invalid default, the valid prefix stays reset while the
    /// failing entry and remaining entries retain their previous values.
    pub fn osmo_tdefs_reset(table: Option<&mut [TDef<'_>]>) -> Result<(), TDefError> {
        for (index, t) in osmo_tdef_for_each_mut(table)?.iter_mut().enumerate() {
            if !osmo_tdef_val_in_range(t, t.default_val) {
                return Err(TDefError::InvalidDefault {
                    index,
                    id: t.id,
                    value: t.default_val,
                    min: t.min_val,
                    max: t.max_val,
                });
            }
            t.val = t.default_val;
        }
        Ok(())
    }

    /// snprintf-style length and truncation. Empty destinations return the full
    /// length without writing, unlike C's unconditional `buf[0]` write at capacity 0.
    pub fn osmo_tdef_range_str_buf(buf: &mut [u8], t: &TDef<'_>) -> usize {
        let text = if t.max_val == 0 {
            format!("[{} .. inf]", t.min_val)
        } else {
            format!("[{} .. {}]", t.min_val, t.max_val)
        };
        if !buf.is_empty() {
            let copied = text.len().min(buf.len() - 1);
            buf[..copied].copy_from_slice(&text.as_bytes()[..copied]);
            buf[copied] = 0;
        }
        text.len()
    }

    pub fn osmo_tdef_get_state_timeout(
        state: u32,
        timeouts: &[StateTimeout; 32],
    ) -> Result<Option<&StateTimeout>, TDefError> {
        if state >= 32 {
            return Err(TDefError::InvalidState { state });
        }
        let t = &timeouts[state as usize];
        Ok(if t.id == 0 && !t.keep_timer {
            None
        } else {
            Some(t)
        })
    }

    /// Only a null table ends group traversal; a present empty table is a group.
    pub fn osmo_tdef_groups_for_each<'g, 't, 'd>(
        groups: Option<&'g [TDefGroup<'t, 'd>]>,
    ) -> Result<&'g [TDefGroup<'t, 'd>], TDefError> {
        match groups {
            None => Ok(&[]),
            Some(groups) => {
                let end = groups
                    .iter()
                    .position(|g| g.tdefs.is_none())
                    .ok_or(TDefError::MissingGroupTerminator)?;
                Ok(&groups[..end])
            }
        }
    }
}

/// A bounded mutable timeout table validated at construction.
/// Storage is borrowed; callers keep ownership. The terminating entry and any
/// suffix are retained for compatibility, while [`Self::entries`] excludes them.
pub struct TDefTable<'t, 'd> {
    table: &'t mut [TDef<'d>],
}
impl<'t, 'd> TDefTable<'t, 'd> {
    pub fn new(table: &'t mut [TDef<'d>]) -> Result<Self, TDefError> {
        legacy::osmo_tdef_for_each(Some(table))?;
        Ok(Self { table })
    }
    /// Inspect definitions before the terminating entry.
    pub fn entries(&self) -> &[TDef<'d>] {
        legacy::osmo_tdef_for_each(Some(self.table)).expect("validated table")
    }
    pub fn get(&self, id: i32, unit: TDefUnit, fallback: i64) -> Result<u64, TDefError> {
        legacy::osmo_tdef_get(Some(self.table), id, unit, fallback)
    }
    pub fn set(&mut self, id: i32, value: u64, unit: TDefUnit) -> Result<(), TDefError> {
        legacy::osmo_tdef_set(Some(self.table), id, value, unit)
    }
    /// Reset defaults in order; an invalid default retains the completed prefix.
    pub fn reset(&mut self) -> Result<(), TDefError> {
        legacy::osmo_tdefs_reset(Some(self.table))
    }
}
impl TDef<'_> {
    pub fn value_in_range(&self, value: u64) -> bool {
        legacy::osmo_tdef_val_in_range(self, value)
    }
    /// Format the allowed range without a C buffer terminator or truncation.
    pub fn range_string(&self) -> String {
        if self.max_val == 0 {
            format!("[{} .. inf]", self.min_val)
        } else {
            format!("[{} .. {}]", self.min_val, self.max_val)
        }
    }
}
