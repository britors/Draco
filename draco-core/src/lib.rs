//! Toolkit-agnostic engine for Draco: Postgres connections, SSH tunnels, schema
//! introspection queries and local storage. No GTK/GUI dependency lives here —
//! see `draco-gtk` for the frontend.

pub mod assistant;
pub mod connection;
pub mod error;
pub mod manager;
pub mod parser;
pub mod postgres;
pub mod secrets;
pub mod store;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
