//! Veyra filesystem abstraction layer.
//!
//! Empty in Faz 1 (Project Infrastructure) by design: directory reading, CRUD
//! operations, metadata extraction and the GIO/GVfs bridge are implemented in
//! Faz 2 (Dosya Sistemi Çekirdeği) per the Veyra roadmap. This crate exists
//! now to fix the workspace boundary between UI and filesystem logic (Rule 41).

#![forbid(unsafe_code)]
