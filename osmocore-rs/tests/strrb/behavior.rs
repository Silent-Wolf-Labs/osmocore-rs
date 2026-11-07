//! Results the C recording can't hold: inputs where C crashes, and what this API adds.

use osmocore_rs::compat::strrb as compat;
use osmocore_rs::strrb::*;

#[test]
fn a_ring_with_no_capacity_keeps_nothing_but_reports_truncation() {
    let mut ring = StringRing::new(0);
    assert!(!ring.push("short"));
    assert!(ring.push([b'a'; 240]));
    assert!(ring.is_empty());
    assert_eq!(ring.get(0), None);
    assert_eq!(ring.capacity(), 0);
}

#[test]
fn compat_add_with_no_slots_returns_an_error() {
    // C crashes here.
    let mut ring = compat::osmo_strrb_create(0);
    assert_eq!(compat::osmo_strrb_add(&mut ring, b"a"), -22);
    assert_eq!(compat::osmo_strrb_elements(&ring), 0);
}

#[test]
fn preferred_keeps_a_nul_and_compat_stops_at_it() {
    let mut ring = StringRing::new(1);
    ring.push(b"a\0b");
    assert_eq!(ring.get(0), Some(&b"a\0b"[..]));

    let mut ring = compat::osmo_strrb_create(2);
    assert_eq!(compat::osmo_strrb_add(&mut ring, b"a\0b"), 0);
    assert_eq!(compat::osmo_strrb_get_nth(&ring, 0), Some(&b"a"[..]));
}

#[test]
fn compat_text_after_a_nul_does_not_count_towards_the_limit() {
    let mut text = vec![b'a'; 300];
    text[5] = 0;
    let mut ring = compat::osmo_strrb_create(2);
    assert_eq!(compat::osmo_strrb_add(&mut ring, &text), 0);
}

#[test]
fn capacity_has_no_16_bit_limit() {
    let mut ring = StringRing::new(70_000);
    for index in 0..70_001 {
        ring.push(index.to_string());
    }
    assert_eq!(ring.len(), 70_000);
    assert_eq!(ring.get(0), Some(&b"1"[..]));
}

#[test]
fn the_largest_capacity_takes_no_space_up_front() {
    let mut ring = StringRing::new(usize::MAX);
    assert!(ring.is_empty());
    ring.push("a");
    assert_eq!(ring.len(), 1);
}

#[test]
fn strings_iterate_oldest_first_in_either_direction() {
    let mut ring = StringRing::new(2);
    for text in ["a", "b", "c"] {
        ring.push(text);
    }
    let strings: Vec<&[u8]> = ring.iter().collect();
    assert_eq!(strings, [b"b", b"c"]);
    assert_eq!(ring.iter().next_back(), Some(&b"c"[..]));
}

#[test]
fn the_limit_applies_at_exactly_239_bytes() {
    let mut ring = StringRing::new(3);
    assert!(!ring.push([b'a'; MAX_STRING_BYTES]));
    assert!(ring.push([b'b'; MAX_STRING_BYTES + 1]));
    assert_eq!(ring.get(1).map(<[u8]>::len), Some(MAX_STRING_BYTES));
}
