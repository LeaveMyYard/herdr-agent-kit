use super::*;

fn refused(text: &str) -> String {
    parse(text).expect_err(text)
}

#[test]
fn refusals_say_what_and_where() {
    let tail = ", which TOML 1.0 (Python's tomllib) refuses";
    for (text, what, at) in [
        (
            "x = { a = 1, }\n",
            "a TOML 1.1 trailing comma in an inline table",
            "1, column 14",
        ),
        (
            "x = 1\ny = {\n  a = 1 }\n",
            "a TOML 1.1 newline in an inline table",
            "2, column 6",
        ),
        (
            "x = 1\r\ny = { a = 1, }\r\n",
            "a TOML 1.1 trailing comma in an inline table",
            "2, column 14",
        ),
        (
            "x = \"ab\\x41\"\n",
            "the TOML 1.1 escape \\x",
            "1, column 8",
        ),
        (
            "x = [ 1979-05-27 07:32 ]\n",
            "a TOML 1.1 time without seconds",
            "1, column 7",
        ),
        ("x = 23:59:60\n", "second 60", "1, column 5"),
        ("x = 0000-01-01\n", "year 0", "1, column 5"),
        ("\u{feff}x = 1\n", "a byte-order mark", "1, column 1"),
        (
            "x = 1\r\r\ny = 2\n",
            "a carriage return not followed by a newline",
            "1, column 6",
        ),
    ] {
        assert_eq!(refused(text), format!("{what}{tail} (at line {at})"));
    }
}

#[test]
fn crlf_reads_as_lf() {
    let table = parse("x = \"\"\"a\r\nb\"\"\"\r\n").unwrap();
    assert_eq!(table["x"].as_str(), Some("a\nb"));
}

#[test]
fn toml10_lookalikes_are_accepted() {
    for text in [
        "x = { a = [\n1, # c\n2,\n] }\n",
        "x = { a = \"\"\"\nline\"\"\" }\n",
        "x = \"\\\\e \\\\x41\"\ny = '\\e'\n",
        "x = 1 # { a = 1, } \\e\n",
        "x = 07:32:00\ny = 1979-05-27 07:32:00Z\nz = \"07:32\"\n",
    ] {
        parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"));
    }
}

#[test]
fn crate_errors_are_one_line() {
    let err = refused("x = [unclosed\n");
    assert!(!err.contains('\n'), "{err}");
    assert!(!err.is_empty());
}

mod goldens;
