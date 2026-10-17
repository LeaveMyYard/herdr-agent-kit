//! `FakeHerdr`, behind the `fake` feature. **Test IO.**
//!
//! A Unix socket server that answers from a scripted state and records every
//! request as `(method, params)`. Integration tests in both plugins use it in
//! place of an argv-recording CLI stub. It is seeded from herdr socket
//! recordings captured on an isolated herdr (see `fixtures/`).
