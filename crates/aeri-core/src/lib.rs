//! Aeri's domain core: identities, tabs, the rules that govern them, and their
//! persistence. Nothing here knows about GPUI or CEF; the engine is reached
//! only through [`engine`]'s commands and events.

pub mod engine;
pub mod ids;
pub mod omnibox;
pub mod profile_db;
pub mod store;
pub mod tab;
pub mod workspace;
