// SPDX-License-Identifier: GPL-3.0-or-later

use super::{TimeError, TimeSpec, TimeVal};
/// Linux clock IDs; unknown numeric IDs remain representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClockId(pub i32);
impl ClockId {
    pub const REALTIME: Self = Self(0);
    pub const MONOTONIC: Self = Self(1);
    pub const PROCESS_CPUTIME: Self = Self(2);
    pub const THREAD_CPUTIME: Self = Self(3);
    pub const MONOTONIC_RAW: Self = Self(4);
    pub const REALTIME_COARSE: Self = Self(5);
    pub const MONOTONIC_COARSE: Self = Self(6);
    pub const BOOTTIME: Self = Self(7);

    fn override_index(self) -> Option<usize> {
        if (0..8).contains(&self.0) {
            Some(self.0 as usize)
        } else {
            None
        }
    }
}

/// Legacy gettimeofday timezone destination, preserved by overridden reads.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimeZone {
    pub minutes_west: i32,
    pub dst_time: i32,
}

/// Independent wall-clock and clock-ID readings. No monotonic substitution.
pub trait Clock {
    fn gettimeofday(&mut self, timezone: Option<&mut TimeZone>) -> Result<TimeVal, TimeError>;
    fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError>;
}

/// Native system clock adapter. Raw types and fake clocks are platform independent.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
mod native {
    use super::*;
    unsafe extern "C" {
        fn gettimeofday(tv: *mut TimeVal, timezone: *mut TimeZone) -> std::os::raw::c_int;
        fn clock_gettime(id: std::os::raw::c_int, tp: *mut TimeSpec) -> std::os::raw::c_int;
    }
    fn error() -> TimeError {
        TimeError::SystemError(std::io::Error::last_os_error().raw_os_error().unwrap_or(5))
    }
    pub fn wall(timezone: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        let mut result = TimeVal::default();
        let tz = timezone.map_or(std::ptr::null_mut(), |tz| tz as *mut TimeZone);
        // SAFETY: On 64-bit Linux timeval has two signed 64-bit fields and
        // timezone two C ints. Both destinations are valid exclusive borrows.
        if unsafe { gettimeofday(&mut result, tz) } == 0 {
            Ok(result)
        } else {
            Err(error())
        }
    }
    pub fn clock(id: ClockId) -> Result<TimeSpec, TimeError> {
        let mut result = TimeSpec::default();
        // SAFETY: timespec is two signed 64-bit fields on this target. The
        // destination is valid; arbitrary clock IDs are checked by the OS.
        if unsafe { clock_gettime(id.0, &mut result) } == 0 {
            Ok(result)
        } else {
            Err(error())
        }
    }
}
impl Clock for SystemClock {
    fn gettimeofday(&mut self, timezone: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
        {
            native::wall(timezone)
        }
        #[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
        {
            let _ = timezone;
            Err(TimeError::UnsupportedPlatform)
        }
    }
    fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError> {
        #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
        {
            native::clock(id)
        }
        #[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
        {
            let _ = id;
            Err(TimeError::UnsupportedPlatform)
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct OverrideSlot {
    enabled: bool,
    time: TimeSpec,
}

/// Owned equivalent of the C wall override globals and eight clock slots.
/// Unknown clock IDs bypass overrides and are passed to the underlying source.
#[derive(Clone, Debug)]
pub struct ClockOverrides<C = SystemClock> {
    pub source: C,
    pub gettimeofday_override: bool,
    pub gettimeofday_override_time: TimeVal,
    clocks: [OverrideSlot; 8],
}
impl<C> ClockOverrides<C> {
    pub fn new(source: C) -> Self {
        Self {
            source,
            gettimeofday_override: false,
            gettimeofday_override_time: TimeVal::new(23, 424_242),
            clocks: [OverrideSlot::default(); 8],
        }
    }
    /// Arithmetic errors leave the stored override unchanged.
    pub fn gettimeofday_override_add(
        &mut self,
        seconds: i64,
        microseconds: i64,
    ) -> Result<(), TimeError> {
        self.gettimeofday_override_time = self
            .gettimeofday_override_time
            .checked_add(TimeVal::new(seconds, microseconds))?;
        Ok(())
    }
    pub fn clock_override_enable(&mut self, id: ClockId, enable: bool) {
        if let Some(index) = id.override_index() {
            self.clocks[index].enabled = enable;
        }
    }
    pub fn clock_override_gettimespec(&self, id: ClockId) -> Option<&TimeSpec> {
        id.override_index().map(|index| &self.clocks[index].time)
    }
    pub fn clock_override_gettimespec_mut(&mut self, id: ClockId) -> Option<&mut TimeSpec> {
        id.override_index()
            .map(|index| &mut self.clocks[index].time)
    }
    pub fn clock_override_add(
        &mut self,
        id: ClockId,
        seconds: i64,
        nanoseconds: i64,
    ) -> Result<(), TimeError> {
        if let Some(index) = id.override_index() {
            self.clocks[index].time = self.clocks[index]
                .time
                .checked_add(TimeSpec::new(seconds, nanoseconds))?;
        }
        Ok(())
    }
}
impl Default for ClockOverrides<SystemClock> {
    fn default() -> Self {
        Self::new(SystemClock)
    }
}
impl<C: Clock> Clock for ClockOverrides<C> {
    fn gettimeofday(&mut self, timezone: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
        if self.gettimeofday_override {
            Ok(self.gettimeofday_override_time)
        } else {
            self.source.gettimeofday(timezone)
        }
    }
    fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError> {
        if let Some(index) = id.override_index()
            && self.clocks[index].enabled
        {
            return Ok(self.clocks[index].time);
        }
        self.source.clock_gettime(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn signed_overflow_is_checked_at_each_c_arithmetic_step() {
        for (a, b, sub) in [
            ((i64::MAX, 0), (1, 0), false),
            ((i64::MIN, 0), (1, 0), true),
            ((i64::MAX, 0), (0, 1_000_000_000), false),
            ((i64::MIN, 0), (0, 1), true),
            ((0, i64::MAX), (0, 1), false),
            ((0, i64::MIN), (0, 1), true),
        ] {
            let (tv_a, tv_b) = (TimeVal::new(a.0, a.1), TimeVal::new(b.0, b.1));
            let (ts_a, ts_b) = (TimeSpec::new(a.0, a.1), TimeSpec::new(b.0, b.1));
            assert_eq!(
                if sub {
                    tv_a.checked_sub(tv_b)
                } else {
                    tv_a.checked_add(tv_b)
                },
                Err(TimeError::ArithmeticOverflow)
            );
            assert_eq!(
                if sub {
                    ts_a.checked_sub(ts_b)
                } else {
                    ts_a.checked_add(ts_b)
                },
                Err(TimeError::ArithmeticOverflow)
            );
        }
        // C overflows the intermediate sum even though mathematical full
        // normalization could eventually bring the fraction back into range.
        assert_eq!(
            TimeVal::new(0, i64::MAX).checked_add(TimeVal::new(0, 1)),
            Err(TimeError::ArithmeticOverflow)
        );
    }

    #[test]
    fn override_arithmetic_errors_preserve_state_and_unknown_ids_are_noops() {
        let mut clocks = ClockOverrides {
            gettimeofday_override_time: TimeVal::new(i64::MAX, 999_999),
            ..Default::default()
        };
        let before = clocks.gettimeofday_override_time;
        assert_eq!(
            clocks.gettimeofday_override_add(0, 1),
            Err(TimeError::ArithmeticOverflow)
        );
        assert_eq!(clocks.gettimeofday_override_time, before);
        *clocks
            .clock_override_gettimespec_mut(ClockId::MONOTONIC)
            .unwrap() = TimeSpec::new(i64::MIN, 0);
        let before = *clocks
            .clock_override_gettimespec(ClockId::MONOTONIC)
            .unwrap();
        assert_eq!(
            clocks.clock_override_add(ClockId::MONOTONIC, -1, 0),
            Err(TimeError::ArithmeticOverflow)
        );
        assert_eq!(
            clocks.clock_override_gettimespec(ClockId::MONOTONIC),
            Some(&before)
        );
        for id in [ClockId(-1), ClockId(8), ClockId(i32::MAX)] {
            clocks.clock_override_enable(id, true);
            assert_eq!(clocks.clock_override_add(id, i64::MAX, i64::MAX), Ok(()));
            assert!(clocks.clock_override_gettimespec_mut(id).is_none());
        }
    }

    #[test]
    fn duration_adapters_check_canonical_fields_and_truncate_only_when_requested() {
        let duration = Duration::new(12, 123_456_789);
        assert_eq!(
            TimeSpec::from_duration(duration).unwrap().to_duration(),
            Ok(duration)
        );
        let tv = TimeVal::from_duration(duration).unwrap();
        assert_eq!(tv, TimeVal::new(12, 123_456));
        assert_eq!(tv.to_duration(), Ok(Duration::new(12, 123_456_000)));
        assert_eq!(
            TimeSpec::new(-1, 0).to_duration(),
            Err(TimeError::NegativeDuration)
        );
        assert_eq!(
            TimeVal::new(-1, 999_999).to_duration(),
            Err(TimeError::NegativeDuration)
        );
        for fraction in [-2_000_001, -1, 1_000_000, 2_000_000] {
            assert_eq!(
                TimeVal::new(0, fraction).to_duration(),
                Err(TimeError::InvalidFraction)
            );
        }
        for fraction in [-2_000_000_001, -1, 1_000_000_000, 2_000_000_000] {
            assert_eq!(
                TimeSpec::new(0, fraction).to_duration(),
                Err(TimeError::InvalidFraction)
            );
        }
        assert_eq!(
            TimeVal::from_duration(Duration::from_secs(u64::MAX)),
            Err(TimeError::ArithmeticOverflow)
        );
        assert_eq!(
            TimeSpec::from_duration(Duration::from_secs(u64::MAX)),
            Err(TimeError::ArithmeticOverflow)
        );
    }

    #[derive(Default)]
    struct Source {
        wall_calls: usize,
        ids: Vec<ClockId>,
        fail: bool,
    }
    impl Clock for Source {
        fn gettimeofday(&mut self, timezone: Option<&mut TimeZone>) -> Result<TimeVal, TimeError> {
            self.wall_calls += 1;
            if self.fail {
                return Err(TimeError::SystemError(5));
            }
            if let Some(tz) = timezone {
                *tz = TimeZone {
                    minutes_west: 3,
                    dst_time: 4,
                };
            }
            Ok(TimeVal::new(100, 200))
        }
        fn clock_gettime(&mut self, id: ClockId) -> Result<TimeSpec, TimeError> {
            self.ids.push(id);
            if self.fail {
                Err(TimeError::SystemError(22))
            } else {
                Ok(TimeSpec::new(300, 400))
            }
        }
    }

    #[test]
    fn disabled_reads_route_to_source_and_overrides_do_not_hide_unrelated_clocks() {
        let mut clocks = ClockOverrides::new(Source::default());
        let mut tz = TimeZone::default();
        assert_eq!(
            clocks.gettimeofday(Some(&mut tz)),
            Ok(TimeVal::new(100, 200))
        );
        assert_eq!(
            tz,
            TimeZone {
                minutes_west: 3,
                dst_time: 4
            }
        );
        assert_eq!(
            clocks.clock_gettime(ClockId::REALTIME),
            Ok(TimeSpec::new(300, 400))
        );
        clocks.gettimeofday_override = true;
        clocks.clock_override_enable(ClockId::MONOTONIC, true);
        clocks.source.fail = true;
        assert_eq!(
            clocks.gettimeofday(Some(&mut tz)),
            Ok(TimeVal::new(23, 424_242))
        );
        assert_eq!(
            tz,
            TimeZone {
                minutes_west: 3,
                dst_time: 4
            }
        );
        assert_eq!(
            clocks.clock_gettime(ClockId::MONOTONIC),
            Ok(TimeSpec::default())
        );
        assert_eq!(
            clocks.clock_gettime(ClockId::REALTIME),
            Err(TimeError::SystemError(22))
        );
        assert_eq!(
            clocks.clock_gettime(ClockId(-123)),
            Err(TimeError::SystemError(22))
        );
        clocks.gettimeofday_override = false;
        clocks.clock_override_enable(ClockId::MONOTONIC, false);
        assert_eq!(clocks.gettimeofday(None), Err(TimeError::SystemError(5)));
        assert_eq!(
            clocks.clock_gettime(ClockId::MONOTONIC),
            Err(TimeError::SystemError(22))
        );
        assert_eq!(clocks.source.wall_calls, 2);
        assert_eq!(
            clocks.source.ids,
            vec![
                ClockId::REALTIME,
                ClockId::REALTIME,
                ClockId(-123),
                ClockId::MONOTONIC
            ]
        );
    }

    #[test]
    fn independent_override_instances_do_not_share_state() {
        let mut first = ClockOverrides::new(Source::default());
        let mut second = ClockOverrides::new(Source::default());
        first.gettimeofday_override = true;
        first.gettimeofday_override_time = TimeVal::new(-4, -5);
        first.clock_override_enable(ClockId::MONOTONIC, true);
        *first
            .clock_override_gettimespec_mut(ClockId::MONOTONIC)
            .unwrap() = TimeSpec::new(-6, -7);
        assert_eq!(first.gettimeofday(None), Ok(TimeVal::new(-4, -5)));
        assert_eq!(
            first.clock_gettime(ClockId::MONOTONIC),
            Ok(TimeSpec::new(-6, -7))
        );
        assert_eq!(second.gettimeofday(None), Ok(TimeVal::new(100, 200)));
        assert_eq!(
            second.clock_gettime(ClockId::MONOTONIC),
            Ok(TimeSpec::new(300, 400))
        );
    }

    #[cfg(all(target_os = "linux", target_pointer_width = "64"))]
    #[test]
    fn native_clock_abi_and_errno_smoke_test_without_sleeps() {
        let mut clock = SystemClock;
        let wall = clock.gettimeofday(None).unwrap();
        assert!((0..1_000_000).contains(&wall.microseconds));
        for id in [ClockId::REALTIME, ClockId::MONOTONIC] {
            let value = clock.clock_gettime(id).unwrap();
            assert!((0..1_000_000_000).contains(&value.nanoseconds));
        }
        assert_eq!(
            clock.clock_gettime(ClockId(i32::MAX)),
            Err(TimeError::SystemError(22))
        );
    }
}
