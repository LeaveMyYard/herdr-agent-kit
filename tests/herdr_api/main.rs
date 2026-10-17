//! `herdr_api` against `FakeHerdr` and `CliStub` only: never a real herdr.
//!
//! The parity tests take each recorded CLI or socket exchange of
//! `fixtures/herdr` and check that the typed call sends exactly the request
//! herdr's own CLI sent, and reads the recorded reply, through both clients.

mod support;

mod cli;
mod events;
mod parity_agents_plugins;
mod parity_tabs_panes;
mod refusals;
mod transport;
