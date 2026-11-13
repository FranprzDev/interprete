//! Captura de audio con cpal: micrófono o audio del sistema (loopback, solo Windows).

use anyhow::{anyhow, bail, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use interprete_core::audio::{to_mono, StreamResampler, SAMPLE_RATE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Micrófono por nombre (None = predeterminado).
    Mic(Option<String>),
    /// Lo que suena por una salida (None = predeterminada). Solo Windows.
    Loopback(Option<String>),
}

pub fn input_names() -> Vec<String> {
    cpal::default_host()
        .input_devices()
        .map(|d| d.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

pub fn output_names() -> Vec<String> {
    cpal::default_host()
        .output_devices()
        .map(|d| d.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

pub fn loopback_supported() -> bool {
    cfg!(windows)
}

fn find_input(name: &Option<String>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match name {
        None => host.default_input_device().ok_or_else(|| anyhow!("no hay micrófono predeterminado")),
        Some(n) => host
            .input_devices()?
            .find(|d| d.name().map(|x| &x == n).unwrap_or(false))
            .ok_or_else(|| anyhow!("no encuentro el micrófono '{n}'")),
    }
}

pub fn find_output(name: &Option<String>) -> Result<cpal::Device> {
    let host = cpal::default_host();
    match name {
        None => host.default_output_device().ok_or_else(|| anyhow!("no hay salida de audio predeterminada")),
        Some(n) => host
            .output_devices()?
            .find(|d| d.name().map(|x| &x == n).unwrap_or(false))
            .ok_or_else(|| anyhow!("no encuentro la salida '{n}'")),
    }
}

fn build<T>(dev: &cpal::Device, cfg: &cpal::StreamConfig, tx: Sender<Vec<f32>>) -> Result<cpal::Stream>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = cfg.channels as usize;
    let mut rs = StreamResampler::new(cfg.sample_rate.0, SAMPLE_RATE);
    let stream = dev.build_input_stream(
        cfg,
        move |data: &[T], _| {
            let f: Vec<f32> = data.iter().map(|s| f32::from_sample(*s)).collect();
            let out = rs.push(&to_mono(&f, channels));
            if !out.is_empty() {
                let _ = tx.send(out);
            }
        },
        |e| eprintln!("error de audio: {e}"),
        None,
    )?;
    Ok(stream)
}

/// Inicia la captura en un hilo propio (los Stream de cpal no son Send).
/// Envía audio mono 16 kHz por `tx` hasta que `stop` sea true.
pub fn spawn_capture(src: Source, tx: Sender<Vec<f32>>, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>> {
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    let h = std::thread::spawn(move || {
        let res: Result<cpal::Stream> = (|| {
            let (dev, supported) = match &src {
                Source::Mic(n) => {
                    let d = find_input(n)?;
                    let c = d.default_input_config()?;
                    (d, c)
                }
                Source::Loopback(n) => {
                    if !loopback_supported() {
                        bail!("capturar el audio del sistema solo funciona en Windows");
                    }
                    let d = find_output(n)?;
                    let c = d.default_output_config()?;
                    (d, c)
                }
            };
            let cfg: cpal::StreamConfig = supported.clone().into();
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => build::<f32>(&dev, &cfg, tx.clone())?,
                cpal::SampleFormat::I16 => build::<i16>(&dev, &cfg, tx.clone())?,
                cpal::SampleFormat::U16 => build::<u16>(&dev, &cfg, tx.clone())?,
                cpal::SampleFormat::I32 => build::<i32>(&dev, &cfg, tx.clone())?,
                f => bail!("formato de audio no soportado: {f:?}"),
            };
            stream.play()?;
            Ok(stream)
        })();
        match res {
            Ok(stream) => {
                let _ = ready_tx.send(Ok(()));
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(100));
                }
                drop(stream);
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
            }
        }
    });
    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(h),
        Ok(Err(e)) => Err(anyhow!(e)),
        Err(_) => Err(anyhow!("el dispositivo de audio no respondió")),
    }
}

/// Reproduce un WAV en la salida elegida (None = predeterminada). Bloquea hasta terminar.
pub fn play_wav(path: &std::path::Path, out: &Option<String>) -> Result<()> {
    let dev = find_output(out)?;
    let (_stream, handle) = rodio::OutputStream::try_from_device(&dev)?;
    let sink = rodio::Sink::try_new(&handle)?;
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    sink.append(rodio::Decoder::new(file)?);
    sink.sleep_until_end();
    Ok(())
}
