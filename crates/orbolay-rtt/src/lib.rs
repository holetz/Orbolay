//! Real-time transcription pipeline. Built only with the root `rtt` feature.

#[cfg(feature = "capture")]
pub mod attribution;
#[cfg(feature = "capture")]
pub mod audio;
#[cfg(feature = "capture")]
pub mod pipeline;
#[cfg(feature = "capture")]
pub mod stt;
