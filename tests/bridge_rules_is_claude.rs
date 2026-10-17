//! "Is this process Claude Code?": the rows of the bridged rules table,
//! `tests/test_bridge_rules.py` in herdr-panorama and herdr-inbox (the same
//! table in both), ported case for case. Each Rust test names the Python test
//! it ports.

use herdr_agent_kit::agent_procs::Pid;
use herdr_agent_kit::agent_procs::claude::{
    claude_process, first_claude, is_claude, nested_claude, owning_claude,
};
use herdr_agent_kit::agent_procs::fake::FakeProcesses;

type Row = (&'static str, &'static [&'static str]);

/// `CLAUDE`
const CLAUDE: &[Row] = &[
    ("claude", &[]), // native install, by command name
    ("claude", &["claude", "--resume"]),
    // versioned binary: comm is the version
    ("2.1.301", &["/home/u/.local/share/claude/versions/2.1.301"]),
    (
        "2.1.301",
        &["/home/u/.local/share/claude/versions/2.1.301", "--continue"],
    ),
    ("MainThread", &["/home/u/.local/bin/claude"]), // a symlink named claude
    (
        "node",
        &[
            "node",
            "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
        ],
    ),
    (
        "node",
        &[
            "/usr/bin/node",
            "/home/u/.npm-global/lib/node_modules/@anthropic-ai/claude-code/cli.js",
            "-p",
        ],
    ),
    // one option before it
    (
        "node",
        &[
            "node",
            "--no-warnings",
            "/opt/x/@anthropic-ai/claude-code/cli.js",
        ],
    ),
    (
        "node22",
        &["node22", "/x/@anthropic-ai/claude-code/cli.mjs"],
    ),
    // under npx: the package's own entry script, below the launcher
    (
        "node",
        &[
            "/home/u/.nvm/versions/node/v24.12.0/bin/node",
            "/home/u/.npm/_npx/0123456789abcdef/node_modules/@anthropic-ai/claude-code/cli.js",
        ],
    ),
    // Claude Code sets its process title: captured as comm "claude", cmdline "claude"
    ("claude", &["claude"]),
];

/// The npx launcher's npm-cli path (`NPX`).
const NPX: &str = "/home/u/.nvm/versions/node/v24.12.0/lib/node_modules/npm/bin/npx-cli.js";

/// `NPX_LAUNCHER`: node running npm's npx-cli.js with the package NAME; after
/// start-up npm retitles it (comm `npm exec @anthr`, 15 bytes).
const NPX_LAUNCHER: &[Row] = &[
    (
        "node",
        &[
            "/home/u/.nvm/versions/node/v24.12.0/bin/node",
            NPX,
            "@anthropic-ai/claude-code",
        ],
    ),
    ("node", &["node", NPX, "-y", "@anthropic-ai/claude-code"]),
    (
        "npm exec @anthr",
        &["npm exec @anthropic-ai/claude-code", "", "", ""],
    ),
];

/// `NOT_CLAUDE`
const NOT_CLAUDE: &[Row] = &[
    ("bash", &["bash", "-c", "claude"]), // a shell that will start claude is not claude
    ("python3", &["python3", "-m", "claude"]),
    ("claude-code", &["claude-code"]),
    ("claudette", &["/usr/bin/claudette"]),
    ("node", &["node", "server.js"]),
    // only the first two args
    (
        "node",
        &["node", "a", "b", "/x/@anthropic-ai/claude-code/cli.js"],
    ),
    ("node", &["node", "@anthropic-ai/claude-code"]), // a bare package name is not the entry script
    ("node", &["node", "/x/@anthropic-ai/claude-code"]),
    ("node", &["node", "/home/u/src/claude-code/cli.js"]), // some other claude-code checkout
    ("node", &["node", "/x/@anthropic-ai/claude-code/cli.js.map"]),
    (
        "node",
        &["node", "/x/@anthropic-ai/claude-code-router/cli.js"],
    ),
    // the scope is a whole path segment
    ("node", &["node", "/x/foo@anthropic-ai/claude-code/cli.js"]),
    ("node", &[]),
    ("uv", &["uv", "run", "/x/@anthropic-ai/claude-code/cli.js"]), // not node
    ("zsh", &["/home/u/.local/share/claude-versions/2.1.301"]),
    ("", &[]),
];

#[test]
fn the_table_has_every_python_row() {
    assert_eq!(
        (CLAUDE.len(), NPX_LAUNCHER.len(), NOT_CLAUDE.len()),
        (11, 3, 16)
    );
}

/// `test_is_claude`
#[test]
fn test_is_claude() {
    for (comm, argv) in CLAUDE {
        assert!(is_claude(comm, argv), "{comm:?} {argv:?} is Claude");
    }
}

/// `test_is_not_claude` (`NOT_CLAUDE + NPX_LAUNCHER`)
#[test]
fn test_is_not_claude() {
    for (comm, argv) in NOT_CLAUDE.iter().chain(NPX_LAUNCHER) {
        assert!(!is_claude(comm, argv), "{comm:?} {argv:?} is not Claude");
    }
}

/// The tree of `test_a_launcher_above_claude_gives_exactly_one_claude`:
/// pid → (parent, comm, start token, argv).
fn launcher_tree() -> FakeProcesses {
    let mut t = FakeProcesses::default();
    t.insert(100, 90, "python3", "s1", &["python3", "-I", "hooks.py"]);
    t.insert(90, 80, "sh", "s2", &["sh", "-c", "..."]);
    t.insert(
        80,
        70,
        "node",
        "s3",
        &[
            "/home/u/.nvm/versions/node/v24.12.0/bin/node",
            "/home/u/.npm/_npx/0123456789abcdef/node_modules/@anthropic-ai/claude-code/cli.js",
        ],
    );
    t.insert(70, 60, "sh", "s4", &["sh", "-c", "claude"]);
    t.insert(
        60,
        50,
        "node",
        "s5",
        &[
            "/home/u/.nvm/versions/node/v24.12.0/bin/node",
            NPX,
            "@anthropic-ai/claude-code",
        ],
    );
    t.insert(50, 1, "zsh", "s6", &["-zsh"]);
    t
}

/// `test_a_launcher_above_claude_gives_exactly_one_claude`, both plugins'
/// versions: npx above the real cli; walking up from a hook (`getppid()` is
/// 100) finds the cli, and the whole chain holds one Claude (the launcher is
/// not a second, outer one).
#[test]
fn test_a_launcher_above_claude_gives_exactly_one_claude() {
    let parent: Pid = 100;
    let mut tree = launcher_tree();
    // panorama: hooks.claude_process; inbox: proc.owning_claude / nested_claude
    assert_eq!(claude_process(&tree, parent), Some((80, "s3".to_owned())));
    assert_eq!(owning_claude(&tree, parent), Some(80));
    assert!(!nested_claude(&tree, parent));
    assert_eq!(tree.claudes(), [80]);

    // retitled launcher
    tree.insert(
        60,
        50,
        "npm exec @anthr",
        "s5",
        &["npm exec @anthropic-ai/claude-code", "", ""],
    );
    assert_eq!(tree.claudes(), [80]);
    assert!(!nested_claude(&tree, parent));

    // no cli below the launcher: no Claude at all
    tree.insert(80, 70, "bash", "s3", &["bash"]);
    assert_eq!(claude_process(&tree, parent), None);
    // the inbox falls back to the grandparent (panorama: None)
    assert_eq!(owning_claude(&tree, parent), Some(90));
    assert!(!nested_claude(&tree, parent));
}

/// `test_herdr_claude_pid_skips_the_launcher` (panorama `herdr.claude_pid`,
/// over a pane's foreground processes; the herdr call itself is
/// `herdr_api`'s).
#[test]
fn test_herdr_claude_pid_skips_the_launcher() {
    let launcher: &[&str] = &["node", NPX, "@anthropic-ai/claude-code"];
    let cli: &[&str] = &["node", "/x/node_modules/@anthropic-ai/claude-code/cli.js"];
    let procs = [(60, "node", launcher), (80, "node", cli)];
    assert_eq!(first_claude(procs), Some(80));
    assert_eq!(first_claude(procs[..1].iter().copied()), None);
}
