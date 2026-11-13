//! Núcleo de Intérprete: audio -> texto -> traducción -> voz.
//! Todo detrás de traits para poder cambiar de motor (whisper, LFM, etc).

pub mod audio;
pub mod engines;
pub mod lang;
pub mod pipeline;
pub mod stt;
pub mod translate;
pub mod tts;
pub mod vad;

pub use lang::Lang;
