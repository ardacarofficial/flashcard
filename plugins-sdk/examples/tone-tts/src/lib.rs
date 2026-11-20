//! Example: a `TtsProvider` in a plugin. Synthesizes 16-bit mono PCM WAV: 40 ms of sine
//! per character (at most 2 s), pitch by voice, speed by `rate`.

use fc_plugin_sdk::prelude::*;
use fc_plugin_sdk::{AudioClip, SpeechRequest, Voice};

pub const SAMPLE_RATE: u32 = 8000;

#[derive(Default)]
pub struct Tone;

fn voices() -> Vec<Voice> {
    vec![
        Voice {
            id: "a4".into(),
            name: "A4".into(),
            language: "en".into(),
        },
        Voice {
            id: "e4".into(),
            name: "E4".into(),
            language: "tr".into(),
        },
    ]
}

/// WAV bytes for `text`.
pub fn wav(text: &str, hz: f32, rate: f32) -> Vec<u8> {
    let chars = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    let ms = ((chars as f32 * 40.0 / rate.clamp(0.25, 4.0)) as u32).min(2000);
    let samples = SAMPLE_RATE * ms / 1000;
    let data_len = samples * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..samples {
        let t = i as f32 / SAMPLE_RATE as f32;
        let v = (t * hz * std::f32::consts::TAU).sin() * 0.3 * i16::MAX as f32;
        out.extend_from_slice(&(v as i16).to_le_bytes());
    }
    out
}

impl Plugin for Tone {
    fn tts_voices(&self, _provider: &str) -> Result<Vec<Voice>> {
        Ok(voices())
    }

    fn tts_synthesize(&self, _provider: &str, request: SpeechRequest) -> Result<AudioClip> {
        if request.text.trim().is_empty() {
            return Err(Error::new("EMPTY_TEXT", "nothing to say"));
        }
        let voice = request
            .voice
            .clone()
            .or_else(|| {
                voices()
                    .into_iter()
                    .find(|v| request.language.starts_with(&v.language))
                    .map(|v| v.id)
            })
            .unwrap_or_else(|| "a4".into());
        let hz = if voice == "e4" { 329.63 } else { 440.0 };
        Ok(AudioClip {
            mime: "audio/wav".into(),
            bytes: wav(&request.text, hz, request.rate),
        })
    }
}

export_plugin!(Tone);
