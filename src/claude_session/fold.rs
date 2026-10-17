//! The session fold. **Pure:** takes records and the current time, returns
//! data; no IO, no threads, no environment.
//!
//! `Session::apply` folds one record into the keyed stores (spawns, acks,
//! notices, resumes, journals, background tasks). `Session::status_of` applies
//! the status rules, taking the clock and a `Liveness` trait as arguments. The
//! continuation rule (D6) lives here: a continuation is cleared only by a later
//! `user` or `assistant` record.
