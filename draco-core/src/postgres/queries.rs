//! All introspection, DDL, dashboard/stats and administration queries. Every function takes a
//! `&PostgresDriver` and returns typed rows — no SQL leaks past this module. Split into one file
//! per feature area (below); `helpers` holds the row-decoding/identifier-quoting functions
//! shared across all of them, `pub(super)` so siblings can use them without making them part of
//! this crate's public API.

mod helpers;

mod introspection;
mod erd;
mod browse_edit;
mod alter_table;
mod column_stats;
mod global_search;
mod explain_plan;
mod dashboard;
mod db_stats;
mod activity_locks;
mod query_stats;
mod sequences;
mod roles;
mod extensions;
mod function_editor;
mod object_editor;
mod cron;

pub use introspection::*;
pub use erd::*;
pub use browse_edit::*;
pub use alter_table::*;
pub use column_stats::*;
pub use global_search::*;
pub use explain_plan::*;
pub use dashboard::*;
pub use db_stats::*;
pub use activity_locks::*;
pub use query_stats::*;
pub use sequences::*;
pub use roles::*;
pub use extensions::*;
pub use function_editor::*;
pub use object_editor::*;
pub use cron::*;
