//! Traducción de texto. Sirve cualquier servidor compatible con la API de chat de OpenAI
//! (llama-server de llama.cpp, LM Studio, Ollama, un modelo Liquid LFM, etc).

use crate::Lang;
use anyhow::{anyhow, Result};
use serde_json::json;

pub trait Translator: Send + Sync {
    fn name(&self) -> String;
    fn translate(&self, text: &str, from: Lang, to: Lang) -> Result<String>;
}

pub struct ChatTranslator {
    pub base_url: String,
    pub model: String,
}

impl ChatTranslator {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), model: model.into() }
    }
}

pub fn system_prompt(from: Lang, to: Lang) -> String {
    format!(
        "You are a professional simultaneous interpreter. Translate the user's message from {} to {}. \
         It is everyday spoken language: keep the tone natural and keep names, numbers and technical terms. \
         Output ONLY the translation, with no notes, quotes or explanations.",
        from.english_name(),
        to.english_name()
    )
}

pub fn clean_translation(raw: &str) -> String {
    let t = raw.trim();
    let t = t.trim_matches(|c| c == '"' || c == '«' || c == '»' || c == '“' || c == '”');
    // Algunos modelos con razonamiento emiten <think>...</think>
    let t = match t.rfind("</think>") {
        Some(i) => &t[i + 8..],
        None => t,
    };
    t.trim().to_string()
}

impl Translator for ChatTranslator {
    fn name(&self) -> String {
        format!("llama.cpp ({})", self.model)
    }

    fn translate(&self, text: &str, from: Lang, to: Lang) -> Result<String> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }
        let body = json!({
            "model": self.model,
            "temperature": 0.0,
            "max_tokens": 400,
            "stream": false,
            "messages": [
                {"role": "system", "content": system_prompt(from, to)},
                {"role": "user", "content": text},
            ],
        });
        let resp = ureq::post(&format!("{}/v1/chat/completions", self.base_url))
            .timeout(std::time::Duration::from_secs(120))
            .send_json(body)
            .map_err(|e| anyhow!("servidor de traducción: {e}"))?;
        let v: serde_json::Value = serde_json::from_reader(resp.into_reader())?;
        let out = v["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("respuesta sin contenido: {v}"))?;
        Ok(clean_translation(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_mentions_langs() {
        let p = system_prompt(Lang::Es, Lang::En);
        assert!(p.contains("Spanish") && p.contains("English"));
    }

    #[test]
    fn cleans_quotes_and_think() {
        assert_eq!(clean_translation("\"Hello there\"\n"), "Hello there");
        assert_eq!(clean_translation("<think>x</think>\nHi"), "Hi");
    }
}
