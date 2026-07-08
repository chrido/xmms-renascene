//! Playback model and backend boundaries.
//!
//! The current desktop app still uses `crate::player` directly for GStreamer,
//! but frontend-neutral code can depend on this module as the backend boundary
//! evolves.

pub mod backend;
#[cfg(feature = "gstreamer-backend")]
pub mod gstreamer;
pub mod model;
#[cfg(feature = "rodio-backend")]
pub mod rodio;
