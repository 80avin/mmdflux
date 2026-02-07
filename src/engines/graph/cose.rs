//! COSE (Compound Spring Embedder) layout engine scaffold.
//!
//! This module reserves the integration seam for a future COSE-Bilkent
//! layout engine. No layout algorithm is implemented; requesting COSE
//! returns a clear "not yet implemented" error.
//!
//! When COSE is implemented, it will be feature-gated behind `engine-cose`.

// No adapter struct is defined until COSE is implemented.
// The `LayoutEngineId::Cose` variant and `check_available()` in `diagram.rs`
// handle all current behavior (recognized but unavailable).
