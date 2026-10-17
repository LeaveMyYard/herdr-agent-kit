use super::*;

#[test]
fn short_sha1_matches_python_hashlib() {
    // python3 -c 'import hashlib; print(hashlib.sha1(s.encode()).hexdigest()[:12])'
    assert_eq!(short_sha1(b"default"), "7505d64a54e0");
    assert_eq!(short_sha1(b"/run/herdr.sock"), "837fc5258150");
    assert_eq!(short_sha1(b""), "da39a3ee5e6b");
}
