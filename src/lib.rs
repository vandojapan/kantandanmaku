//! Deterministic danmaku filter prototype for AviUtl2.
//!
//! The simulation/runtime modules are intentionally usable without AviUtl2 so
//! they can be tested on the host platform.  The actual `.auf2` adapter is
//! compiled behind the `aviutl2-plugin` feature.

pub mod danmaku;
pub mod render;
pub mod script;

#[cfg(feature = "aviutl2-plugin")]
pub mod filter;
