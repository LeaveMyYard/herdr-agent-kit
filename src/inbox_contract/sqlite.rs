//! Read-only reader of the inbox database, behind the `sqlite` feature. **IO.**
//!
//! Uses only the original 0.1 `questions` columns, opens read-only, and treats
//! a missing file, a lock or a missing table as "no inbox", never as an error.
