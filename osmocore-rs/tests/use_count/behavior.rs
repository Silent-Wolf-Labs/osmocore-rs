//! Results the C recording can't hold: inputs where C overflows or crashes, and what
//! this API adds.

use osmocore_rs::compat::use_count as compat;
use osmocore_rs::use_count::*;

#[test]
fn a_change_of_int_min_on_a_negative_count_is_out_of_range() {
    // C's negation overflows here.
    let mut uses = UseCount::new();
    uses.put("a").unwrap();
    assert_eq!(uses.change("a", i32::MIN), Err(CountOutOfRange));
    assert_eq!(uses.count_of("a"), -1);
}

#[test]
fn the_total_stops_at_the_limit_with_int_min_after_a_negative_total() {
    // C's negation overflows here.
    let mut uses = UseCount::new();
    uses.put("a").unwrap();
    uses.change("b", i32::MIN).unwrap();
    assert_eq!(uses.total(), i32::MIN);
}

#[test]
fn idle_entries_can_be_removed() {
    // C forbids this for static entries.
    let mut uses = UseCount::new();
    uses.add_idle_entries(2);
    assert!(uses.remove(None));
    assert_eq!(uses.entries().count(), 1);
    assert!(uses.remove(None));
    assert!(!uses.remove(None));
    assert_eq!(uses.to_string(), "0 (-)");
}

#[test]
fn a_zero_change_reports_the_count_and_changes_nothing() {
    let mut uses = UseCount::new();
    let change = uses.change("a", 0).unwrap();
    assert_eq!((change.old, change.new), (0, 0));
    assert_eq!(uses.entries().count(), 0);
    assert_eq!(uses.to_string(), "0 ()");

    uses.change("a", 2).unwrap();
    let change = uses.change("a", 0).unwrap();
    assert_eq!((change.entry, change.old, change.new), (0, 2, 2));
}

#[test]
fn a_change_gives_the_position_of_its_entry() {
    let mut uses = UseCount::new();
    uses.add_idle_entries(2);
    let first = uses.get(None).unwrap();
    assert_eq!(first.entry, 1);
    assert_eq!(uses.entries().nth(first.entry), Some((None, 1)));
    // The other idle entry now comes first, and it's the one a None token finds.
    assert_eq!(uses.count_of(None), 0);
    let second = uses.get("a").unwrap();
    assert_eq!(uses.entries().nth(second.entry), Some((Some("a"), 1)));
}

#[test]
fn entry_positions_past_the_end_are_refused() {
    let mut uses = UseCount::new();
    assert!(!uses.set_entry_count(0, 1));
    assert!(!uses.remove_entry(0));
}

#[test]
fn compat_runs_no_callback_for_a_zero_or_failed_change() {
    let mut runs = 0;
    let mut callback = |_: &mut UseCount, _: UseChange| {
        runs += 1;
        0
    };
    let mut uses = UseCount::new();
    assert_eq!(
        compat::osmo_use_count_get_put(Some(&mut uses), Some("a"), 0, Some(&mut callback)),
        0
    );
    uses.change("a", i32::MAX).unwrap();
    assert_eq!(
        compat::osmo_use_count_get_put(Some(&mut uses), Some("a"), 1, Some(&mut callback)),
        -34
    );
    assert_eq!(runs, 0);
}

#[test]
fn compat_name_buf_returns_its_buffer() {
    let mut uses = UseCount::new();
    uses.get("a").unwrap();
    let mut buffer = [0xaa; 8];
    assert_eq!(
        compat::osmo_use_count_name_buf(&mut buffer, &uses),
        b"1 (a)\0\xaa\xaa"
    );
    assert_eq!(compat::osmo_use_count_name_buf(&mut [], &uses), b"");
}

#[test]
fn the_error_has_a_message() {
    assert_eq!(CountOutOfRange.to_string(), "use count out of range");
}
