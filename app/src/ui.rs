use crate::capture::{self, Source};
use interprete_core::{
    engines::{self, Progress, Servers},
    pipeline::{Pipeline, Utterance},
    tts::{SystemTts, Tts},
    vad::{Segmenter, VadConfig},
    Lang,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone)]
enum Phase {
    Checking,
    NeedInstall,
    Installing(Progress),
    Starting,
    Ready,
    Error(String),
}

struct Line {
    lane: usize,
    original: String,
    translated: String,
}

struct Shared {
    phase: Phase,
    pipeline: Option<Arc<Pipeline>>,
    _servers: Option<Servers>,
    lines: Vec<Line>,
    status: [String; 2],
}

type SharedRef = Arc<Mutex<Shared>>;

struct LaneUi {
    from: Lang,
    to: Lang,
    title: &'static str,
    active: bool,
    mic: Option<String>,
    loopback: bool,
    speak: bool,
    out: Option<String>,
    stop: Option<Arc<AtomicBool>>,
    threads: Vec<JoinHandle<()>>,
    error: Option<String>,
}

impl LaneUi {
    fn new(title: &'static str, from: Lang, to: Lang, loopback: bool, speak: bool) -> Self {
        Self {
            from,
            to,
            title,
            active: false,
            mic: None,
            loopback,
            speak,
            out: None,
            stop: None,
            threads: Vec::new(),
            error: None,
        }
    }

    fn stop(&mut self) {
        if let Some(s) = self.stop.take() {
            s.store(true, Ordering::Relaxed);
        }
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

pub struct App {
    shared: SharedRef,
    lanes: [LaneUi; 2],
    inputs: Vec<String>,
    outputs: Vec<String>,
    ctx: eframe::egui::Context,
    subtitles: bool,
}

fn set_phase(s: &SharedRef, ctx: &eframe::egui::Context, p: Phase) {
    s.lock().unwrap().phase = p;
    ctx.request_repaint();
}

fn start_engines(shared: SharedRef, ctx: eframe::egui::Context) {
    set_phase(&shared, &ctx, Phase::Starting);
    match engines::start(crate::threads()) {
        Ok(servers) => {
            let mut g = shared.lock().unwrap();
            g._servers = Some(servers);
            g.pipeline = Some(Arc::new(crate::build_pipeline()));
            g.phase = Phase::Ready;
            drop(g);
            ctx.request_repaint();
        }
        Err(e) => set_phase(&shared, &ctx, Phase::Error(format!("{e:#}"))),
    }
}

fn run_install(shared: SharedRef, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let (s2, c2) = (shared.clone(), ctx.clone());
        let r = engines::install(&mut |p| set_phase(&s2, &c2, Phase::Installing(p)));
        match r {
            Ok(()) => start_engines(shared, ctx),
            Err(e) => set_phase(&shared, &ctx, Phase::Error(format!("{e:#}"))),
        }
    });
}

fn lane_worker(
    idx: usize,
    cfg: (Lang, Lang, Source, bool, Option<String>),
    pipeline: Arc<Pipeline>,
    shared: SharedRef,
    ctx: eframe::egui::Context,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<Vec<JoinHandle<()>>> {
    let (from, to, source, speak, out) = cfg;
    let (tx, rx) = channel::<Vec<f32>>();
    let cap = capture::spawn_capture(source, tx, stop.clone())?;
    let st = stop.clone();
    let worker = std::thread::spawn(move || {
        let mut seg = Segmenter::new(VadConfig::default());
        let set = |t: &str| {
            shared.lock().unwrap().status[idx] = t.to_string();
            ctx.request_repaint();
        };
        set("Escuchando...");
        let tts = SystemTts;
        let tmp = std::env::temp_dir().join(format!("interprete-tts-{idx}.wav"));
        while !st.load(Ordering::Relaxed) {
            let chunk = match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            for phrase in seg.push(&chunk) {
                set("Traduciendo...");
                match pipeline.process(&phrase, from, to) {
                    Ok(Some(u)) => {
                        let Utterance { original, translated, .. } = u;
                        shared.lock().unwrap().lines.push(Line {
                            lane: idx,
                            original,
                            translated: translated.clone(),
                        });
                        ctx.request_repaint();
                        if speak && !translated.is_empty() {
                            set("Hablando...");
                            if tts.synth_to_wav(&translated, to, &tmp).is_ok() {
                                let _ = capture::play_wav(&tmp, &out);
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => set(&format!("Error: {e:#}")),
                }
                set("Escuchando...");
            }
        }
        shared.lock().unwrap().status[idx] = "Detenido".into();
        ctx.request_repaint();
    });
    Ok(vec![cap, worker])
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            phase: Phase::Checking,
            pipeline: None,
            _servers: None,
            lines: Vec::new(),
            status: [String::new(), String::new()],
        }));
        let ctx = cc.egui_ctx.clone();
        let mut visuals = eframe::egui::Visuals::dark();
        visuals.window_rounding = 8.0.into();
        ctx.set_visuals(visuals);
        {
            let (s, c) = (shared.clone(), ctx.clone());
            std::thread::spawn(move || {
                if engines::is_installed() {
                    start_engines(s, c);
                } else {
                    set_phase(&s, &c, Phase::NeedInstall);
                }
            });
        }
        Self {
            shared,
            lanes: [
                LaneUi::new("Yo hablo (español a inglés)", Lang::Es, Lang::En, false, true),
                LaneUi::new("Escucho (inglés a español)", Lang::En, Lang::Es, capture::loopback_supported(), false),
            ],
            inputs: capture::input_names(),
            outputs: capture::output_names(),
            ctx,
            subtitles: false,
        }
    }

    fn toggle_lane(&mut self, i: usize) {
        let pipeline = self.shared.lock().unwrap().pipeline.clone();
        let l = &mut self.lanes[i];
        if !l.active {
            l.stop();
            self.shared.lock().unwrap().status[i].clear();
            return;
        }
        let Some(p) = pipeline else {
            l.active = false;
            return;
        };
        let source = if l.loopback { Source::Loopback(None) } else { Source::Mic(l.mic.clone()) };
        let stop = Arc::new(AtomicBool::new(false));
        match lane_worker(
            i,
            (l.from, l.to, source, l.speak, l.out.clone()),
            p,
            self.shared.clone(),
            self.ctx.clone(),
            stop.clone(),
        ) {
            Ok(t) => {
                l.stop = Some(stop);
                l.threads = t;
                l.error = None;
            }
            Err(e) => {
                l.active = false;
                l.error = Some(format!("{e:#}"));
            }
        }
    }
}

fn device_combo(ui: &mut eframe::egui::Ui, id: &str, sel: &mut Option<String>, names: &[String]) {
    let text = sel.clone().unwrap_or_else(|| "Predeterminado".into());
    eframe::egui::ComboBox::from_id_salt(id).selected_text(text).width(280.0).show_ui(ui, |ui| {
        ui.selectable_value(sel, None, "Predeterminado");
        for n in names {
            ui.selectable_value(sel, Some(n.clone()), n);
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
        use eframe::egui::{self, RichText};
        let phase = self.shared.lock().unwrap().phase.clone();
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Intérprete");
            ui.label("Traducción simultánea español e inglés. Todo corre en tu PC, gratis.");
            ui.add_space(8.0);
            match phase {
                Phase::Checking => {
                    ui.spinner();
                }
                Phase::NeedInstall => {
                    ui.label("Primera vez: hay que descargar los motores y modelos (unos 1,7 GB, una sola vez).");
                    ui.add_space(6.0);
                    if ui.add_sized([260.0, 44.0], egui::Button::new(RichText::new("Instalar").size(20.0))).clicked() {
                        run_install(self.shared.clone(), self.ctx.clone());
                        set_phase(&self.shared, &self.ctx, Phase::Installing(Progress::default()));
                    }
                }
                Phase::Installing(p) => {
                    ui.label(format!("Paso {} de {}: {}", p.step.max(1), p.steps.max(4), p.label));
                    let (frac, txt) = match p.total {
                        Some(t) if t > 0 => (
                            p.done as f32 / t as f32,
                            format!("{} de {} MB", p.done / 1_000_000, t / 1_000_000),
                        ),
                        _ => (0.0, format!("{} MB", p.done / 1_000_000)),
                    };
                    ui.add(egui::ProgressBar::new(frac).text(txt).desired_width(480.0));
                    ui.label("No cierres la ventana. Si se corta, volvé a abrir y sigue donde quedó.");
                }
                Phase::Starting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Iniciando motores (la primera vez tarda unos segundos)...");
                    });
                }
                Phase::Error(e) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("Algo falló: {e}"));
                    if ui.button("Reintentar").clicked() {
                        if engines::is_installed() {
                            let (s, c) = (self.shared.clone(), self.ctx.clone());
                            std::thread::spawn(move || start_engines(s, c));
                        } else {
                            run_install(self.shared.clone(), self.ctx.clone());
                        }
                    }
                }
                Phase::Ready => {
                    for i in 0..2 {
                        ui.separator();
                        let status = self.shared.lock().unwrap().status[i].clone();
                        let mut changed = false;
                        let l = &mut self.lanes[i];
                        ui.horizontal(|ui| {
                            changed |= ui.checkbox(&mut l.active, RichText::new(l.title).strong().size(16.0)).changed();
                            ui.label(status);
                        });
                        ui.add_enabled_ui(!l.active, |ui| {
                            ui.horizontal(|ui| {
                                if capture::loopback_supported() && i == 1 {
                                    ui.radio_value(&mut l.loopback, true, "Audio del sistema (Zoom, Meet, video)");
                                    ui.radio_value(&mut l.loopback, false, "Micrófono");
                                }
                                if !l.loopback {
                                    ui.label("Micrófono:");
                                    device_combo(ui, &format!("mic{i}"), &mut l.mic, &self.inputs);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut l.speak, "Decir la traducción en voz alta");
                                if l.speak {
                                    ui.label("Salida:");
                                    device_combo(ui, &format!("out{i}"), &mut l.out, &self.outputs);
                                }
                            });
                        });
                        if let Some(e) = &l.error {
                            ui.colored_label(egui::Color32::LIGHT_RED, e);
                        }
                        if changed {
                            self.toggle_lane(i);
                        }
                    }
                    ui.separator();
                    ui.checkbox(&mut self.subtitles, "Subtítulos grandes (ventana aparte, siempre visible)");
                    let g = self.shared.lock().unwrap();
                    egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                        for ln in &g.lines {
                            let tag = if ln.lane == 0 { "ES a EN" } else { "EN a ES" };
                            ui.label(RichText::new(format!("{tag}: {}", ln.original)).weak());
                            ui.label(RichText::new(&ln.translated).size(24.0));
                            ui.add_space(6.0);
                        }
                    });
                }
            }
        });
        if self.subtitles {
            let last = {
                let g = self.shared.lock().unwrap();
                g.lines.last().map(|l| l.translated.clone()).unwrap_or_default()
            };
            let mut close = false;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("subtitulos"),
                egui::ViewportBuilder::default()
                    .with_title("Subtítulos")
                    .with_always_on_top()
                    .with_inner_size([900.0, 200.0]),
                |ctx, _| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new(&last).size(44.0).strong());
                        });
                    });
                    if ctx.input(|i| i.viewport().close_requested()) {
                        close = true;
                    }
                },
            );
            if close {
                self.subtitles = false;
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for l in &mut self.lanes {
            l.stop();
        }
    }
}

pub fn run() {
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([820.0, 680.0]).with_title("Intérprete"),
        ..Default::default()
    };
    if let Err(e) = eframe::run_native("Intérprete", opts, Box::new(|cc| Ok(Box::new(App::new(cc))))) {
        eprintln!("error: {e}");
    }
}
