//! Transcript IO. **IO.**
//!
//! Resolves the projects root (`${CLAUDE_CONFIG_DIR:-~/.claude}/projects`),
//! discovers transcript files and tails them with `Tailer` (`Tail::Missing`,
//! `NoChange`, `Reset` or `Lines`, with an 8 MB cap on a partial line). Parsed
//! records are handed to [`super::fold`].
