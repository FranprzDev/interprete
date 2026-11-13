//! Descarga, instala y arranca los motores locales (whisper-server y llama-server).
//! Todo queda en una carpeta de datos del usuario; no se toca nada más del sistema.

use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const WHISPER_PORT: u16 = 18710;
pub const LLAMA_PORT: u16 = 18711;

const STT_TAG: &str = "https://github.com/FranprzDev/interprete/releases/download/engines-v1";
const LLAMA_TAG: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b12340";
pub const WHISPER_MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin";
pub const WHISPER_MODEL_FILE: &str = "ggml-small.bin";
pub const LLM_MODEL_URL: &str =
    "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf";
pub const LLM_MODEL_FILE: &str = "qwen2.5-1.5b-instruct-q4_k_m.gguf";

#[derive(Clone, Debug)]
pub struct Asset {
    pub label: &'static str,
    pub url: String,
    pub file: String,
    /// Subcarpeta donde se extrae (vacío = es un archivo suelto).
    pub extract_to: Option<&'static str>,
}

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub step: usize,
    pub steps: usize,
    pub label: String,
    pub done: u64,
    pub total: Option<u64>,
}

pub fn data_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("Interprete")
}

fn platform_key() -> Result<(&'static str, &'static str)> {
    // (sufijo del servidor STT, sufijo de llama.cpp)
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok(("windows-x64.zip", "win-cpu-x64.zip")),
        ("linux", "x86_64") => Ok(("linux-x64.tar.gz", "ubuntu-x64.tar.gz")),
        ("macos", "aarch64") => Ok(("macos-arm64.tar.gz", "macos-arm64.tar.gz")),
        ("macos", "x86_64") => Ok(("macos-x64.tar.gz", "macos-x64.tar.gz")),
        (os, arch) => bail!("plataforma no soportada: {os}-{arch}"),
    }
}

pub fn assets() -> Result<Vec<Asset>> {
    let (stt, llama) = platform_key()?;
    Ok(vec![
        Asset {
            label: "Motor de voz (whisper)",
            url: format!("{STT_TAG}/interprete-whisper-{stt}"),
            file: format!("interprete-whisper-{stt}"),
            extract_to: Some("whisper"),
        },
        Asset {
            label: "Motor de traducción (llama.cpp)",
            url: format!("{LLAMA_TAG}/llama-b12340-bin-{llama}"),
            file: format!("llama-b12340-bin-{llama}"),
            extract_to: Some("llama"),
        },
        Asset {
            label: "Modelo de voz (whisper small, ~470 MB)",
            url: WHISPER_MODEL_URL.into(),
            file: WHISPER_MODEL_FILE.into(),
            extract_to: None,
        },
        Asset {
            label: "Modelo de traducción (Qwen 1.5B, ~1,1 GB)",
            url: LLM_MODEL_URL.into(),
            file: LLM_MODEL_FILE.into(),
            extract_to: None,
        },
    ])
}

fn model_path(file: &str) -> PathBuf {
    data_dir().join("models").join(file)
}

fn find_binary(dir: &Path, stems: &[&str]) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                let n = n.to_lowercase();
                if stems.iter().any(|s| n == *s || n == format!("{s}.exe")) {
                    return Some(p);
                }
            }
        }
    }
    None
}

pub fn whisper_bin() -> Option<PathBuf> {
    find_binary(&data_dir().join("whisper"), &["whisper-server"])
}
pub fn llama_bin() -> Option<PathBuf> {
    find_binary(&data_dir().join("llama"), &["llama-server"])
}

pub fn is_installed() -> bool {
    whisper_bin().is_some()
        && llama_bin().is_some()
        && model_path(WHISPER_MODEL_FILE).exists()
        && model_path(LLM_MODEL_FILE).exists()
}

fn download(url: &str, dest: &Path, on: &mut dyn FnMut(u64, Option<u64>)) -> Result<()> {
    if let Some(p) = dest.parent() {
        fs::create_dir_all(p)?;
    }
    let part = dest.with_extension("part");
    let resp = ureq::get(url).call().map_err(|e| anyhow!("no se pudo descargar {url}: {e}"))?;
    let total = resp.header("Content-Length").and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_reader();
    let mut f = fs::File::create(&part)?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    let mut last = Instant::now();
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
        done += n as u64;
        if last.elapsed() > Duration::from_millis(150) {
            on(done, total);
            last = Instant::now();
        }
    }
    f.flush()?;
    drop(f);
    if let Some(t) = total {
        if done != t {
            bail!("descarga incompleta ({done} de {t} bytes)");
        }
    }
    fs::rename(&part, dest)?;
    on(done, total);
    Ok(())
}

fn extract(archive: &Path, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let name = archive.to_string_lossy().to_lowercase();
    if name.ends_with(".zip") {
        let mut z = zip::ZipArchive::new(fs::File::open(archive)?)?;
        z.extract(dir)?;
    } else {
        let gz = flate2::read::GzDecoder::new(fs::File::open(archive)?);
        tar::Archive::new(gz).unpack(dir)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d)?.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(m) = fs::metadata(&p) {
                    let mut perm = m.permissions();
                    perm.set_mode(perm.mode() | 0o755);
                    let _ = fs::set_permissions(&p, perm);
                }
            }
        }
    }
    Ok(())
}

/// Descarga e instala lo que falte. Se puede llamar de nuevo si se corta (retoma lo ya descargado).
pub fn install(progress: &mut dyn FnMut(Progress)) -> Result<()> {
    let list = assets()?;
    let n = list.len();
    let root = data_dir();
    fs::create_dir_all(&root)?;
    for (i, a) in list.iter().enumerate() {
        let (target, done_marker): (PathBuf, PathBuf) = match a.extract_to {
            Some(sub) => (root.join("downloads").join(&a.file), root.join(sub).join(".ok")),
            None => (model_path(&a.file), model_path(&a.file)),
        };
        if done_marker.exists() {
            progress(Progress { step: i + 1, steps: n, label: format!("{} (ya instalado)", a.label), done: 1, total: Some(1) });
            continue;
        }
        let label = a.label.to_string();
        let mut cb = |d: u64, t: Option<u64>| {
            progress(Progress { step: i + 1, steps: n, label: label.clone(), done: d, total: t })
        };
        download(&a.url, &target, &mut cb).with_context(|| format!("al descargar: {}", a.label))?;
        if let Some(sub) = a.extract_to {
            progress(Progress { step: i + 1, steps: n, label: format!("Descomprimiendo: {}", a.label), done: 0, total: None });
            let out = root.join(sub);
            extract(&target, &out).with_context(|| format!("al descomprimir: {}", a.label))?;
            fs::write(out.join(".ok"), b"ok")?;
            let _ = fs::remove_file(&target);
        }
    }
    Ok(())
}

pub struct Servers {
    children: Vec<Child>,
}

impl Drop for Servers {
    fn drop(&mut self) {
        for c in &mut self.children {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn spawn(bin: &Path, args: &[String]) -> Result<Child> {
    let mut cmd = Command::new(bin);
    cmd.args(args)
        .current_dir(bin.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        // Que las .so junto al binario se encuentren.
        if let Some(d) = bin.parent() {
            cmd.env("LD_LIBRARY_PATH", d).env("DYLD_LIBRARY_PATH", d);
        }
    }
    cmd.spawn().with_context(|| format!("no se pudo iniciar {}", bin.display()))
}

fn wait_port(port: u16, child: &mut Child, what: &str, max: Duration) -> Result<()> {
    let t0 = Instant::now();
    while t0.elapsed() < max {
        if let Ok(Some(status)) = child.try_wait() {
            bail!("{what} se cerró al iniciar ({status})");
        }
        if TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse()?, Duration::from_millis(300)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    bail!("{what} no respondió en {} s", max.as_secs())
}

/// Espera a que llama-server termine de cargar el modelo (/health = 200).
fn wait_llama_ready(port: u16, max: Duration) -> Result<()> {
    let t0 = Instant::now();
    while t0.elapsed() < max {
        if ureq::get(&format!("http://127.0.0.1:{port}/health")).timeout(Duration::from_secs(3)).call().is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    bail!("el modelo de traducción no terminó de cargar")
}

pub fn start(threads: usize) -> Result<Servers> {
    let w = whisper_bin().ok_or_else(|| anyhow!("falta el motor de voz: instalá primero"))?;
    let l = llama_bin().ok_or_else(|| anyhow!("falta el motor de traducción: instalá primero"))?;
    let t = threads.max(1).to_string();
    let mut servers = Servers { children: Vec::new() };

    let mut wc = spawn(
        &w,
        &[
            "-m".into(),
            model_path(WHISPER_MODEL_FILE).to_string_lossy().into(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            WHISPER_PORT.to_string(),
            "-t".into(),
            t.clone(),
        ],
    )?;
    let r = wait_port(WHISPER_PORT, &mut wc, "El motor de voz", Duration::from_secs(120));
    servers.children.push(wc);
    r?;

    let mut lc = spawn(
        &l,
        &[
            "-m".into(),
            model_path(LLM_MODEL_FILE).to_string_lossy().into(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            LLAMA_PORT.to_string(),
            "-c".into(),
            "2048".into(),
            "-t".into(),
            t,
        ],
    )?;
    let r = wait_port(LLAMA_PORT, &mut lc, "El motor de traducción", Duration::from_secs(120));
    servers.children.push(lc);
    r?;
    wait_llama_ready(LLAMA_PORT, Duration::from_secs(180))?;
    Ok(servers)
}

pub fn whisper_url() -> String {
    format!("http://127.0.0.1:{WHISPER_PORT}")
}
pub fn llama_url() -> String {
    format!("http://127.0.0.1:{LLAMA_PORT}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_resolve_on_this_platform() {
        let a = assets().unwrap();
        assert_eq!(a.len(), 4);
        assert!(a[0].url.contains("interprete-whisper"));
        assert!(a[1].url.contains("llama-b12340"));
    }

    #[test]
    fn find_binary_recurses() {
        let d = std::env::temp_dir().join(format!("int-test-{}", std::process::id()));
        let sub = d.join("a/b");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("llama-server.exe"), b"x").unwrap();
        assert!(find_binary(&d, &["llama-server"]).is_some());
        assert!(find_binary(&d, &["whisper-server"]).is_none());
        let _ = fs::remove_dir_all(&d);
    }
}
