//! Not a ported file: the stand-in for an engine not yet ported, so the voice registers all 24
//! in Plaits' order and the ported engines' arena offsets are already their own. It allocates
//! nothing and renders silence; its post-processing settings are what its registration says.
//! Each goes when its engine is ported (the crate's `AGENTS.md`, *Work Guidance*).

use super::engine::{Engine, EngineParameters, PostProcessingSettings};
use crate::stmlib::utils::buffer_allocator::{Arena, BufferAllocator};
use crate::stmlib::utils::random::Random;

#[derive(Default)]
pub struct PlaceholderEngine {
    pub post_processing_settings: PostProcessingSettings,
}

impl PlaceholderEngine {
    /// `ChiptuneEngine::NO_ENVELOPE`, which the voice passes the chiptune engine.
    pub const NO_ENVELOPE: i32 = 2;

    /// `ChiptuneEngine::set_envelope_shape`, which the voice calls: nothing to set yet.
    pub fn set_envelope_shape(&mut self, _envelope_shape: f32) {}

    /// `SpeechEngine::set_prosody_amount`, which the voice calls: nothing to set yet.
    pub fn set_prosody_amount(&mut self, _prosody_amount: f32) {}

    /// `SpeechEngine::set_speed`, which the voice calls: nothing to set yet.
    pub fn set_speed(&mut self, _speed: f32) {}
}

impl Engine for PlaceholderEngine {
    fn init(&mut self, _allocator: &mut BufferAllocator<'_>) {}

    fn reset(&mut self, _arena: &mut Arena) {}

    fn load_user_data(&mut self, _user_data: Option<&'static [u8]>, _arena: &mut Arena) {}

    fn render(
        &mut self,
        _parameters: &EngineParameters,
        out: &mut [f32],
        aux: &mut [f32],
        size: usize,
        _already_enveloped: &mut bool,
        _arena: &mut Arena,
        _random: &mut Random,
    ) {
        out[..size].fill(0.0);
        aux[..size].fill(0.0);
    }

    fn post_processing_settings(&self) -> &PostProcessingSettings {
        &self.post_processing_settings
    }

    fn post_processing_settings_mut(&mut self) -> &mut PostProcessingSettings {
        &mut self.post_processing_settings
    }
}
