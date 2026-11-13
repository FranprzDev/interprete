import { CDN, STT, MT, LANG_NAME, cleanTranscript } from "./config.js";

let tf, asr, mt = {};
const files = new Map();

function report(label) {
  let loaded = 0, total = 0;
  for (const f of files.values()) { loaded += f.loaded; total += f.total; }
  postMessage({ type: "progress", label, loaded, total });
}
const cb = (label) => (p) => {
  if (p.status === "progress" && p.total) { files.set(p.file, { loaded: p.loaded, total: p.total }); report(label); }
  else if (p.status === "done" && files.has(p.file)) { const f = files.get(p.file); f.loaded = f.total; report(label); }
};

async function load({ stt, webgpu }) {
  tf = await import(CDN);
  // Los modelos se bajan por el espejo del mismo sitio (/hf/) para evitar bloqueos de CORS.
  if (self.location.protocol.startsWith("http") && !self.location.hostname.startsWith("localhost")) tf.env.remoteHost = self.location.origin + "/hf/";
  postMessage({ type: "status", text: "Descargando el modelo de voz..." });
  const id = STT[stt].id;
  try {
    if (!webgpu) throw new Error("sin webgpu");
    asr = await tf.pipeline("automatic-speech-recognition", id, {
      device: "webgpu",
      dtype: { encoder_model: "fp32", decoder_model_merged: "q4" },
      progress_callback: cb("Modelo de voz"),
    });
    postMessage({ type: "device", device: "webgpu" });
  } catch (e) {
    files.clear();
    asr = await tf.pipeline("automatic-speech-recognition", id, { dtype: "q8", progress_callback: cb("Modelo de voz") });
    postMessage({ type: "device", device: "wasm" });
  }
  postMessage({ type: "status", text: "Descargando traductores..." });
  for (const l of ["es", "en"]) {
    mt[l] = await tf.pipeline("translation", MT[l], { dtype: "q8", progress_callback: cb("Traductor") });
  }
  postMessage({ type: "ready" });
}

async function process({ id, audio, from, to }) {
  const t0 = performance.now();
  const out = await asr(audio, { language: LANG_NAME[from], task: "transcribe" });
  const original = cleanTranscript(out.text);
  const sttMs = performance.now() - t0;
  if (!original) return postMessage({ type: "result", id, empty: true });
  const t1 = performance.now();
  const tr = await mt[from](original, { max_new_tokens: 200 });
  postMessage({ type: "result", id, original, translated: tr[0].translation_text, sttMs, mtMs: performance.now() - t1 });
}

onmessage = async (e) => {
  try {
    if (e.data.type === "load") await load(e.data);
    else if (e.data.type === "process") await process(e.data);
  } catch (err) {
    postMessage({ type: "error", message: String(err && err.message || err) });
  }
};
