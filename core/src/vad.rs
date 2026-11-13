//! Detector de frases por energía: corta cuando hay una pausa de silencio.

use crate::audio::SAMPLE_RATE;

pub struct VadConfig {
    /// Umbral RMS para considerar voz.
    pub threshold: f32,
    /// Silencio (segundos) que cierra una frase.
    pub silence_s: f32,
    /// Duración mínima de frase (segundos); más corto se descarta.
    pub min_s: f32,
    /// Duración máxima: se corta aunque no haya pausa.
    pub max_s: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self { threshold: 0.012, silence_s: 0.7, min_s: 0.5, max_s: 12.0 }
    }
}

pub struct Segmenter {
    cfg: VadConfig,
    buf: Vec<f32>,
    in_speech: bool,
    silent_run: usize,
    pending: Vec<f32>, // restos de bloque no alineado
}

const BLOCK: usize = (SAMPLE_RATE as usize) / 50; // 20 ms

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

impl Segmenter {
    pub fn new(cfg: VadConfig) -> Self {
        Self { cfg, buf: Vec::new(), in_speech: false, silent_run: 0, pending: Vec::new() }
    }

    /// Alimenta audio mono 16 kHz. Devuelve las frases completas detectadas.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut out = Vec::new();
        self.pending.extend_from_slice(samples);
        let n_blocks = self.pending.len() / BLOCK;
        let data: Vec<f32> = self.pending.drain(..n_blocks * BLOCK).collect();
        for block in data.chunks_exact(BLOCK) {
            let voiced = rms(block) >= self.cfg.threshold;
            if voiced {
                self.in_speech = true;
                self.silent_run = 0;
                self.buf.extend_from_slice(block);
            } else if self.in_speech {
                self.silent_run += BLOCK;
                self.buf.extend_from_slice(block);
                if self.silent_run as f32 / SAMPLE_RATE as f32 >= self.cfg.silence_s {
                    self.finish(&mut out);
                }
            }
            if self.in_speech && self.buf.len() as f32 / SAMPLE_RATE as f32 >= self.cfg.max_s {
                self.finish(&mut out);
            }
        }
        out
    }

    /// Cierra lo que haya en curso (al detener).
    pub fn flush(&mut self) -> Option<Vec<f32>> {
        let mut out = Vec::new();
        self.finish(&mut out);
        out.pop()
    }

    fn finish(&mut self, out: &mut Vec<Vec<f32>>) {
        let dur = self.buf.len() as f32 / SAMPLE_RATE as f32;
        let speech = self.buf.len().saturating_sub(self.silent_run) as f32 / SAMPLE_RATE as f32;
        if speech >= self.cfg.min_s && dur > 0.0 {
            out.push(std::mem::take(&mut self.buf));
        } else {
            self.buf.clear();
        }
        self.in_speech = false;
        self.silent_run = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(sec: f32, amp: f32) -> Vec<f32> {
        (0..(sec * SAMPLE_RATE as f32) as usize).map(|i| (i as f32 * 0.3).sin() * amp).collect()
    }

    #[test]
    fn splits_on_silence() {
        let mut s = Segmenter::new(VadConfig::default());
        let mut audio = Vec::new();
        audio.extend(tone(1.0, 0.3));
        audio.extend(vec![0.0; 16_000]);
        audio.extend(tone(1.2, 0.3));
        audio.extend(vec![0.0; 16_000]);
        let segs = s.push(&audio);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn drops_short_blips() {
        let mut s = Segmenter::new(VadConfig::default());
        let mut audio = tone(0.1, 0.3);
        audio.extend(vec![0.0; 16_000]);
        assert!(s.push(&audio).is_empty());
    }

    #[test]
    fn cuts_at_max() {
        let mut s = Segmenter::new(VadConfig::default());
        let segs = s.push(&tone(13.0, 0.3));
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn flush_returns_partial() {
        let mut s = Segmenter::new(VadConfig::default());
        assert!(s.push(&tone(1.0, 0.3)).is_empty());
        assert!(s.flush().is_some());
    }
}
