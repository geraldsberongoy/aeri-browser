//! Aeri's domain core: identities, tabs, the rules that govern them, and their
//! persistence. Nothing here knows about GPUI or CEF; the engine is reached
//! only through [`engine`]'s commands and events.

pub mod appearance;
pub mod credentials;
pub mod engine;
pub mod ids;
pub mod keymap;
pub mod layout;
pub mod omnibox;
pub mod profile_db;
pub mod resources;
pub mod store;
pub mod suggest;
pub mod tab;
pub mod trust;
pub mod workspace;
