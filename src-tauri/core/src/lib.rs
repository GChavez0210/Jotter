//! Vault logic with no window or webview in it: the Jotter app and the `jotter` command-line
//! tool both build on this crate, so a note is read, indexed, and written the same way by each.

pub mod agent;
pub mod fs_atomic;
pub mod health;
pub mod index;
pub mod link_update;
pub mod links;
pub mod md_scan;
pub mod plain;
pub mod platform;
pub mod recycle;
pub mod search;
pub mod tasks;
pub mod types;
pub mod vault;
pub mod versions;
