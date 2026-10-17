//! [`CliStub`]: a fake `herdr` binary for the CLI path.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::private_dir;
use super::recordings::recordings;

/// What [`CliStub`] does for one argv.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CliReply {
    /// Printed on stdout.
    pub stdout: String,
    /// Printed on stderr.
    pub stderr: String,
    /// The exit status.
    pub exit: u8,
    /// Slept first (for timeouts).
    pub delay: Option<Duration>,
}

impl CliReply {
    /// Exit 0 with this stdout.
    pub fn ok(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_owned(),
            ..Self::default()
        }
    }
}

/// A fake `herdr` binary: a shell script that answers scripted argv and logs
/// every call. Unscripted argv exits 2 with a message on stderr. Removes its
/// directory when dropped.
pub struct CliStub {
    dir: PathBuf,
    replies: Vec<(Vec<String>, CliReply)>,
}

/// The separator of logged arguments: a control character, which
/// [`clean`](crate::herdr_api::clean)ed arguments never contain.
const SEP: char = '\u{1f}';

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

impl CliStub {
    /// A stub with nothing scripted.
    pub fn new() -> Self {
        let stub = Self {
            dir: private_dir("hc"),
            replies: Vec::new(),
        };
        Self::install(&stub.dir);
        stub.write();
        stub
    }

    /// A stub that answers every CLI record of the recordings (its argv
    /// after `herdr`) with the recorded stdout, stderr and exit status.
    pub fn recorded() -> Self {
        let mut stub = Self::new();
        for (_, r) in recordings() {
            if r["kind"] != "cli" {
                continue;
            }
            let argv: Vec<String> = r["argv"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .skip(1)
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            let reply = CliReply {
                stdout: r["stdout"].as_str().unwrap_or_default().to_owned(),
                stderr: r["stderr"].as_str().unwrap_or_default().to_owned(),
                exit: r["exit_code"]
                    .as_u64()
                    .and_then(|c| u8::try_from(c).ok())
                    .unwrap_or(1),
                delay: None,
            };
            stub.replies.retain(|(a, _)| *a != argv);
            stub.replies.push((argv, reply));
        }
        stub.write();
        stub
    }

    /// Answer `args` (after `herdr`) with `reply` from now on.
    pub fn on(&mut self, args: &[&str], reply: CliReply) -> &mut Self {
        let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        self.replies.retain(|(a, _)| *a != args);
        self.replies.push((args, reply));
        self.write();
        self
    }

    /// The stub's path, for `with_bin`.
    pub fn bin(&self) -> PathBuf {
        self.dir.join("herdr")
    }

    /// Every call so far: its arguments after `herdr`.
    pub fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(|l| {
                l.strip_suffix(SEP)
                    .map(|l| l.split(SEP).map(str::to_owned).collect())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The executable, written once: it sources `rules.sh`, which
    /// [`CliStub::on`] rewrites. Rewriting a file that is exec'd races with
    /// other threads' forks (`ETXTBSY`); a sourced file is only read.
    fn install(dir: &Path) {
        let rules = sh_quote(&dir.join("rules.sh").to_string_lossy());
        let path = dir.join("herdr");
        std::fs::write(&path, format!("#!/bin/sh\n. {rules}\n")).expect("write the stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod the stub");
    }

    fn write(&self) {
        let log = sh_quote(&self.dir.join("calls.log").to_string_lossy());
        let mut script = format!(
            "key=$(for a in \"$@\"; do printf '%s\\037' \"$a\"; done)\n\
             printf '%s\\n' \"$key\" >> {log}\n\
             case \"$key\" in\n"
        );
        for (args, reply) in &self.replies {
            let key: String = args.iter().map(|a| format!("{a}{SEP}")).collect();
            script.push_str(&format!("  {})\n", sh_quote(&key)));
            if let Some(d) = reply.delay {
                script.push_str(&format!("    sleep {:.3}\n", d.as_secs_f64()));
            }
            script.push_str(&format!(
                "    printf '%s' {}\n    printf '%s' {} >&2\n    exit {}\n    ;;\n",
                sh_quote(&reply.stdout),
                sh_quote(&reply.stderr),
                reply.exit
            ));
        }
        script.push_str("esac\necho \"fake herdr: nothing scripted for: $*\" >&2\nexit 2\n");
        let tmp = self.dir.join("rules.tmp");
        std::fs::write(&tmp, script).expect("write the stub's rules");
        std::fs::rename(&tmp, self.dir.join("rules.sh")).expect("install the stub's rules");
    }
}

impl Default for CliStub {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CliStub {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
