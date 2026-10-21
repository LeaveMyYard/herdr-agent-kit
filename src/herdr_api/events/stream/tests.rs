//! The reconnect backoff (panorama tests/test_follow.py
//! `test_r12_l1_backoff_grows_until_data_arrives`): each connection that
//! ends or fails waits longer, up to the cap.

use super::*;

#[test]
fn the_backoff_grows_until_it_is_capped() {
    let mut events = Events::new(PathBuf::from("unused"), "t", &[]);
    let mut waits = Vec::new();
    for _ in 0..7 {
        waits.push(events.backoff.as_secs_f64());
        events.back_off();
    }
    assert_eq!(waits, [0.5, 1.0, 2.0, 4.0, 8.0, 10.0, 10.0]); // never stuck at 0.5
}
