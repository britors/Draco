//! All introspection, DDL, dashboard/stats and administration queries. Every function takes a
//! `&PostgresDriver` and returns typed rows — no SQL leaks past this module. Split into one file
//! per feature area (below); `helpers` holds the row-decoding/identifier-quoting functions
//! shared across all of them, `pub(super)` so siblings can use them without making them part of
//! this crate's public API.

mod helpers;

mod activity_locks;
mod alter_table;
mod browse_edit;
mod column_stats;
mod cron;
mod dashboard;
mod db_stats;
mod erd;
mod explain_plan;
mod extensions;
mod function_editor;
mod global_search;
mod introspection;
mod object_editor;
mod query_stats;
mod roles;
mod schema_snapshot;
mod sequences;

pub use activity_locks::*;
pub use alter_table::*;
pub use browse_edit::*;
pub use column_stats::*;
pub use cron::*;
pub use dashboard::*;
pub use db_stats::*;
pub use erd::*;
pub use explain_plan::*;
pub use extensions::*;
pub use function_editor::*;
pub use global_search::*;
pub use introspection::*;
pub use object_editor::*;
pub use query_stats::*;
pub use roles::*;
pub use schema_snapshot::*;
pub use sequences::*;
