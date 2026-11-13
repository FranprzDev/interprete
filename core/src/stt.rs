//! Voz a texto. `Stt` es el punto de enchufe para cambiar de motor.

use crate::Lang;
use anyhow::{anyhow, Result};

pub trait Stt: Send + Sync {
    fn name(&self) -> String;
    /// `wav` es un WAV PCM16 mono 16 kHz.
    fn transcribe(&self, wav: &[u8], lang: Lang) -> Result<String>;
}

/// Cliente del `whisper-server` de whisper.cpp (endpoint /inference).
pub struct WhisperServer {
    pub base_url: String,
}

impl WhisperServer {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into() }
    }
}

fn multipart(boundary: &str, fields: &[(&str, &str)], file: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    for (k, v) in fields {
        b.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes(),
        );
    }
    b.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    b.extend_from_slice(file);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

impl Stt for WhisperServer {
    fn name(&self) -> String {
        "whisper.cpp".into()
    }

    fn transcribe(&self, wav: &[u8], lang: Lang) -> Result<String> {
        let boundary = "----interprete7d3f9a";
        let body = multipart(
            boundary,
            &[("temperature", "0.0"), ("response_format", "json"), ("language", lang.code())],
            wav,
        );
        let resp = ureq::post(&format!("{}/inference", self.base_url))
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .timeout(std::time::Duration::from_secs(120))
            .send_bytes(&body)
            .map_err(|e| anyhow!("whisper-server: {e}"))?;
        let v: serde_json::Value = serde_json::from_reader(resp.into_reader())?;
        let text = v["text"].as_str().ok_or_else(|| anyhow!("respuesta sin 'text': {v}"))?;
        Ok(clean_transcript(text))
    }
}

/// Limpia marcas de whisper ("[BLANK_AUDIO]", "(música)") y alucinaciones típicas de silencio.
pub fn clean_transcript(raw: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in raw.chars() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => {
                if depth > 0 {
                    depth -= 1
                }
            }
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let low = t.to_lowercase();
    const JUNK: &[&str] = &[
        "gracias por ver el video",
        "subtítulos realizados por la comunidad de amara.org",
        "thanks for watching",
        "thank you for watching",
    ];
    if JUNK.iter().any(|j| low.trim_matches(|c: char| !c.is_alphanumeric()) == *j) {
        return String::new();
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_marks() {
        assert_eq!(clean_transcript(" [BLANK_AUDIO] "), "");
        assert_eq!(clean_transcript(" Hola (música) que tal \n"), "Hola que tal");
        assert_eq!(clean_transcript("Thanks for watching!"), "");
    }

    #[test]
    fn multipart_has_file() {
        let b = multipart("B", &[("language", "es")], b"RIFF");
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("name=\"language\"\r\n\r\nes"));
        assert!(s.contains("filename=\"audio.wav\""));
        assert!(s.ends_with("--B--\r\n"));
    }
}
