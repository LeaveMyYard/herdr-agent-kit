use super::*;

#[test]
fn session_file_name_is_the_canonical_uuid_rule() {
    let id = "1CD70DE7-aaaa-4bbb-8ccc-0123456789AB";
    assert_eq!(
        session_file_name(id).as_deref(),
        Some("1cd70de7-aaaa-4bbb-8ccc-0123456789ab")
    );
    for bad in [
        "",
        "../x",
        "*",
        "1cd70de7-aaaa-4bbb-8ccc-0123456789ab ",
        " 1cd70de7-aaaa-4bbb-8ccc-0123456789ab",
        "1cd70de7-aaaa-4bbb-8ccc-0123456789ab-",
        "1cd70de7-aaaa-4bbb-8ccc-0123456789a",
        "1cd70de7aaaa4bbb8ccc0123456789ab",
        "1cd70de7-aaaa-4bbb-8ccc-0123456789ag",
        "1cd70de7-aaaa-4bbb-8ccc/0123456789ab",
        "\u{ff11}cd70de7-aaaa-4bbb-8ccc-0123456789ab",
    ] {
        assert_eq!(session_file_name(bad), None, "{bad:?}");
    }
}

#[test]
fn short_sha1_matches_python_hashlib() {
    // python3 -c 'import hashlib; print(hashlib.sha1(s.encode()).hexdigest()[:12])'
    assert_eq!(short_sha1(b"default"), "7505d64a54e0");
    assert_eq!(short_sha1(b"/run/herdr.sock"), "837fc5258150");
    assert_eq!(short_sha1(b""), "da39a3ee5e6b");
}
