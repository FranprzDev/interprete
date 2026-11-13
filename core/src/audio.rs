//! Utilidades de audio: mezcla a mono, remuestreo a 16 kHz y WAV en memoria.

use anyhow::Result;
use std::io::Cursor;

pub const SAMPLE_RATE: u32 = 16_000;

/// Convierte audio intercalado de `channels` canales a mono promediando.
pub fn to_mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return data.to_vec();
    }
    data.chunks_exact(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Remuestreo simple (promedio de cajas al bajar, lineal al subir). Suficiente para voz.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = (input.len() as f64 / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let start = i as f64 * ratio;
        let end = ((i + 1) as f64 * ratio).min(input.len() as f64);
        if ratio > 1.0 {
            let s = start.floor() as usize;
            let e = (end.ceil() as usize).max(s + 1).min(input.len());
            let sum: f32 = input[s..e].iter().sum();
            out.push(sum / (e - s) as f32);
        } else {
            let s = start.floor() as usize;
            let frac = (start - s as f64) as f32;
            let a = input[s];
            let b = *input.get(s + 1).unwrap_or(&a);
            out.push(a + (b - a) * frac);
        }
    }
    out
}

/// Codifica mono f32 a WAV PCM16 en memoria.
pub fn wav_bytes(samples: &[f32], rate: u32) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cur = Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cur, spec)?;
        for s in samples {
            w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
        }
        w.finalize()?;
    }
    Ok(cur.into_inner())
}

/// Lee un WAV cualquiera (PCM16/PCM32/float) y devuelve mono 16 kHz.
pub fn read_wav_16k_mono(bytes: &[u8]) -> Result<Vec<f32>> {
    let mut r = hound::WavReader::new(Cursor::new(bytes))?;
    let spec = r.spec();
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<_, _>>()?
        }
    };
    Ok(resample(&to_mono(&samples, ch), spec.sample_rate, SAMPLE_RATE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_mix() {
        assert_eq!(to_mono(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
    }

    #[test]
    fn resample_lengths() {
        let x = vec![0.1f32; 48_000];
        assert_eq!(resample(&x, 48_000, 16_000).len(), 16_000);
        let y = vec![0.1f32; 8_000];
        assert_eq!(resample(&y, 8_000, 16_000).len(), 16_000);
    }

    #[test]
    fn wav_roundtrip() {
        let x: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let b = wav_bytes(&x, 16_000).unwrap();
        let y = read_wav_16k_mono(&b).unwrap();
        assert_eq!(y.len(), x.len());
        assert!((y[100] - x[100]).abs() < 0.001);
    }
}

/// Remuestreador en streaming a 16 kHz (promedio por cajas), para usar dentro del callback de captura.
pub struct StreamResampler {
    ratio: f64,
    pos: f64,
    buf: Vec<f32>,
}

impl StreamResampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { ratio: from as f64 / to as f64, pos: 0.0, buf: Vec::new() }
    }

    pub fn push(&mut self, input: &[f32]) -> Vec<f32> {
        self.buf.extend_from_slice(input);
        let mut out = Vec::new();
        loop {
            let end = self.pos + self.ratio;
            let e = end.ceil() as usize;
            if e >= self.buf.len() {
                break;
            }
            let s = self.pos.floor() as usize;
            let slice = &self.buf[s..e.max(s + 1)];
            out.push(slice.iter().sum::<f32>() / slice.len() as f32);
            self.pos = end;
        }
        let consumed = self.pos.floor() as usize;
        if consumed > 0 {
            self.buf.drain(..consumed);
            self.pos -= consumed as f64;
        }
        out
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[test]
    fn streaming_matches_rate() {
        let mut r = StreamResampler::new(44_100, 16_000);
        let mut total = 0;
        for _ in 0..100 {
            total += r.push(&vec![0.2f32; 441]).len();
        }
        // 100 * 441 muestras = 1 s a 44.1 kHz -> ~16000 a 16 kHz
        assert!((total as i64 - 16_000).abs() < 10, "total={total}");
    }

    #[test]
    fn passthrough_values() {
        let mut r = StreamResampler::new(48_000, 16_000);
        let o = r.push(&vec![0.5f32; 480]);
        assert!(o.iter().all(|v| (v - 0.5).abs() < 1e-6));
    }
}
