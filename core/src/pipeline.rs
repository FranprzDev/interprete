//! Una frase de audio -> texto original + traducción.

use crate::{audio, stt::Stt, translate::Translator, Lang};
use anyhow::Result;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Utterance {
    pub from: Lang,
    pub to: Lang,
    pub original: String,
    pub translated: String,
    pub stt_ms: u128,
    pub translate_ms: u128,
}

pub struct Pipeline {
    pub stt: Box<dyn Stt>,
    pub translator: Box<dyn Translator>,
}

impl Pipeline {
    pub fn new(stt: Box<dyn Stt>, translator: Box<dyn Translator>) -> Self {
        Self { stt, translator }
    }

    /// `samples`: mono 16 kHz. Devuelve None si no se entendió nada.
    pub fn process(&self, samples: &[f32], from: Lang, to: Lang) -> Result<Option<Utterance>> {
        let wav = audio::wav_bytes(samples, audio::SAMPLE_RATE)?;
        let t0 = Instant::now();
        let original = self.stt.transcribe(&wav, from)?;
        let stt_ms = t0.elapsed().as_millis();
        if original.trim().is_empty() {
            return Ok(None);
        }
        let t1 = Instant::now();
        let translated = self.translator.translate(&original, from, to)?;
        Ok(Some(Utterance { from, to, original, translated, stt_ms, translate_ms: t1.elapsed().as_millis() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeStt(&'static str);
    impl Stt for FakeStt {
        fn name(&self) -> String {
            "fake".into()
        }
        fn transcribe(&self, _: &[u8], _: Lang) -> Result<String> {
            Ok(self.0.into())
        }
    }
    struct Upper;
    impl Translator for Upper {
        fn name(&self) -> String {
            "upper".into()
        }
        fn translate(&self, t: &str, _: Lang, _: Lang) -> Result<String> {
            Ok(t.to_uppercase())
        }
    }

    #[test]
    fn runs_both_stages() {
        let p = Pipeline::new(Box::new(FakeStt("hola")), Box::new(Upper));
        let u = p.process(&[0.1; 16_000], Lang::Es, Lang::En).unwrap().unwrap();
        assert_eq!(u.original, "hola");
        assert_eq!(u.translated, "HOLA");
    }

    #[test]
    fn empty_transcript_is_none() {
        let p = Pipeline::new(Box::new(FakeStt("")), Box::new(Upper));
        assert!(p.process(&[0.1; 16_000], Lang::Es, Lang::En).unwrap().is_none());
    }
}
