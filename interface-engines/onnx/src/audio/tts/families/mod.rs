/// TTS Family Handler Modules
///
/// Each module corresponds to one of the 8 families defined in the
/// TTS Family Taxonomy (02_TTS_FAMILY_TAXONOMY_AND_CONTRACTS.md).
/// Working families delegate to existing handler implementations.
/// Unimplemented families return descriptive errors with pipeline requirements.

pub mod vits_piper;
pub mod kokoro;
pub mod matcha;
pub mod cosyvoice;
pub mod supertonic;
pub mod audio8;
pub mod chatterbox;
pub mod omnivoice;
