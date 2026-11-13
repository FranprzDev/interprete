import { Resampler, Segmenter } from "./vad.js";

const $ = (id) => document.getElementById(id);
let worker, ready = false, mode = "es"; // es: hablo español -> inglés; en: escucho inglés -> español
let listening = false, stream, ctx, node, seg, rs, nextId = 1, busy = 0, speaking = false;

function showError(msg) { $("error").textContent = msg; $("error").hidden = false; }

function startWorker() {
  worker = new Worker("worker.js", { type: "module" });
  worker.onmessage = (e) => {
    const m = e.data;
    if (m.type === "progress") {
      const pct = m.total ? Math.min(100, (m.loaded / m.total) * 100) : 0;
      $("bar").value = pct;
      $("load-label").textContent = m.label;
      $("load-detail").textContent = `${Math.round(m.loaded / 1e6)} de ${Math.round(m.total / 1e6)} MB (${Math.round(pct)}%)`;
    } else if (m.type === "status") {
      $("load-label").textContent = m.text;
      $("bar").value = 0; $("load-detail").textContent = "";
    } else if (m.type === "device") {
      $("device").textContent = m.device === "webgpu" ? "Usando la placa de video (rápido)." : "Usando el procesador (más lento; en Chrome o Edge con placa de video va mejor).";
    } else if (m.type === "ready") {
      ready = true;
      $("start").hidden = true; $("live").hidden = false;
    } else if (m.type === "result") {
      busy--; updateStatus();
      if (m.empty) return;
      show(m);
    } else if (m.type === "error") {
      busy = Math.max(0, busy - 1);
      showError("Algo falló: " + m.message);
      $("btn-start").disabled = false;
    }
  };
}

$("btn-start").onclick = () => {
  $("btn-start").disabled = true; $("error").hidden = true; $("loading").hidden = false;
  if (!worker) startWorker();
  worker.postMessage({ type: "load", stt: $("use-small").checked ? "small" : "base", webgpu: !!navigator.gpu });
};

function setMode(m) {
  mode = m;
  $("mode-es").classList.toggle("on", m === "es");
  $("mode-en").classList.toggle("on", m === "en");
  if (listening) stopListening();
}
$("mode-es").onclick = () => setMode("es");
$("mode-en").onclick = () => setMode("en");

function updateStatus() {
  $("status").textContent = !listening ? "" : busy > 0 ? "Traduciendo..." : "Escuchando...";
}

function show(m) {
  $("subtitle").textContent = m.translated;
  const h = document.createElement("div");
  h.innerHTML = `<div class="orig"></div><div class="tr"></div>`;
  h.children[0].textContent = m.original;
  h.children[1].textContent = m.translated;
  $("history").prepend(h);
  if ($("speak").checked && "speechSynthesis" in window) {
    const u = new SpeechSynthesisUtterance(m.translated);
    u.lang = mode === "es" ? "en-US" : "es-AR";
    speaking = true;
    u.onend = u.onerror = () => { speaking = false; };
    speechSynthesis.speak(u);
  }
}

const WORKLET = `class P extends AudioWorkletProcessor{process(i){const c=i[0][0];if(c)this.port.postMessage(c.slice());return true}}registerProcessor('cap',P)`;

async function startListening() {
  try {
    if ($("tab-audio").checked) {
      stream = await navigator.mediaDevices.getDisplayMedia({ video: true, audio: true });
      if (!stream.getAudioTracks().length) { stream.getTracks().forEach((t) => t.stop()); throw new Error("No elegiste compartir audio. Marcá 'Compartir audio de la pestaña'."); }
      stream.getVideoTracks().forEach((t) => t.stop());
    } else {
      stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: true, noiseSuppression: true } });
    }
  } catch (e) {
    $("status").textContent = e.name === "NotAllowedError" ? "No diste permiso al micrófono. Habilitalo en el candado de la barra de direcciones." : "No pude acceder al audio: " + e.message;
    return;
  }
  ctx = new AudioContext();
  await ctx.audioWorklet.addModule(URL.createObjectURL(new Blob([WORKLET], { type: "application/javascript" })));
  const src = ctx.createMediaStreamSource(stream);
  node = new AudioWorkletNode(ctx, "cap");
  seg = new Segmenter(); rs = new Resampler(ctx.sampleRate);
  node.port.onmessage = (e) => {
    if (speaking) return; // no escucharse a sí mismo mientras habla la voz
    for (const phrase of seg.push(rs.push(e.data))) {
      busy++; updateStatus();
      const from = mode, to = mode === "es" ? "en" : "es";
      worker.postMessage({ type: "process", id: nextId++, audio: phrase, from, to });
    }
  };
  src.connect(node);
  listening = true;
  $("btn-listen").textContent = "Detener";
  updateStatus();
}

function stopListening() {
  listening = false;
  try { node && node.disconnect(); ctx && ctx.close(); stream && stream.getTracks().forEach((t) => t.stop()); } catch {}
  $("btn-listen").textContent = "Empezar a escuchar";
  updateStatus();
}

$("btn-listen").onclick = () => (listening ? stopListening() : startListening());
$("btn-full").onclick = () => document.documentElement.requestFullscreen?.();
document.onfullscreenchange = () => document.body.classList.toggle("full", !!document.fullscreenElement);

if (!window.isSecureContext || !navigator.mediaDevices) showError("Abrí esta página por https para poder usar el micrófono.");
