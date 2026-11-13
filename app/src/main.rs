#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod capture;
mod ui;

use anyhow::{bail, Result};
use interprete_core::{
    audio, engines,
    pipeline::Pipeline,
    stt::WhisperServer,
    translate::ChatTranslator,
    Lang,
};

pub fn build_pipeline() -> Pipeline {
    Pipeline::new(
        Box::new(WhisperServer::new(engines::whisper_url())),
        Box::new(ChatTranslator::new(engines::llama_url(), "qwen2.5-1.5b-instruct")),
    )
}

pub fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8)
}

fn cli(args: Vec<String>) -> Result<()> {
    match args[0].as_str() {
        "install" => {
            let mut last = 0u64;
            engines::install(&mut |p| {
                let mb = p.done / 1_000_000;
                if mb != last || p.total.map(|t| t == p.done).unwrap_or(false) {
                    last = mb;
                    eprintln!("[{}/{}] {} - {} MB", p.step, p.steps, p.label, mb);
                }
            })?;
            println!("instalado en {}", engines::data_dir().display());
            Ok(())
        }
        // translate-file <es|en> <es|en> a.wav b.wav ...
        "translate-file" => {
            if args.len() < 4 {
                bail!("uso: interprete translate-file <es|en> <es|en> archivo.wav...");
            }
            let from = Lang::parse(&args[1]).ok_or_else(|| anyhow::anyhow!("idioma origen inválido"))?;
            let to = Lang::parse(&args[2]).ok_or_else(|| anyhow::anyhow!("idioma destino inválido"))?;
            let _servers = engines::start(threads())?;
            let p = build_pipeline();
            for f in &args[3..] {
                let samples = audio::read_wav_16k_mono(&std::fs::read(f)?)?;
                match p.process(&samples, from, to)? {
                    Some(u) => println!(
                        "{f}\n  ORIGINAL:   {}\n  TRADUCCION: {}\n  (voz {} ms, traduccion {} ms)",
                        u.original, u.translated, u.stt_ms, u.translate_ms
                    ),
                    None => println!("{f}\n  (no se entendió nada)"),
                }
            }
            Ok(())
        }
        other => bail!("comando desconocido: {other} (usá: install | translate-file)"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        if let Err(e) = cli(args) {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    ui::run();
}
