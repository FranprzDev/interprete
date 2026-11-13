//! Texto a voz con las voces del sistema (gratis, sin descargas):
//! Windows (System.Speech), macOS (`say`), Linux (`espeak-ng`).

use crate::Lang;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

pub trait Tts: Send + Sync {
    /// Genera un WAV con `text` hablado en `lang`.
    fn synth_to_wav(&self, text: &str, lang: Lang, out: &Path) -> Result<()>;
}

pub struct SystemTts;

#[cfg(windows)]
fn hide(cmd: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000)
}
#[cfg(not(windows))]
fn hide(cmd: &mut Command) -> &mut Command {
    cmd
}

impl Tts for SystemTts {
    fn synth_to_wav(&self, text: &str, lang: Lang, out: &Path) -> Result<()> {
        let culture = match lang {
            Lang::En => "en-US",
            Lang::Es => "es-AR",
        };
        let status = if cfg!(windows) {
            let script = "Add-Type -AssemblyName System.Speech; \
                $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
                try { $s.SelectVoiceByHints('NotSet','NotSet',0,[System.Globalization.CultureInfo]$env:INT_CULTURE) } catch {}; \
                $s.SetOutputToWaveFile($env:INT_OUT); $s.Speak($env:INT_TEXT); $s.Dispose()";
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("INT_TEXT", text)
                .env("INT_OUT", out)
                .env("INT_CULTURE", culture);
            hide(&mut c).status()
        } else if cfg!(target_os = "macos") {
            Command::new("say")
                .args(["--data-format=LEI16@22050", "-o"])
                .arg(out)
                .arg(text)
                .status()
        } else {
            Command::new("espeak-ng").args(["-v", lang.code(), "-w"]).arg(out).arg(text).status()
        };
        match status {
            Ok(s) if s.success() && out.exists() => Ok(()),
            Ok(s) => Err(anyhow!("el sintetizador de voz falló ({s})")),
            Err(e) => Err(anyhow!("no se pudo ejecutar el sintetizador de voz: {e}")),
        }
    }
}
