// Configuración compartida entre la web y el test de Node.
export const TRANSFORMERS_VERSION = "4.3.2";
export const CDN = `https://cdn.jsdelivr.net/npm/@huggingface/transformers@${TRANSFORMERS_VERSION}`;

export const STT = {
  base: { id: "onnx-community/whisper-base", label: "Rápida" },
  small: { id: "onnx-community/whisper-small", label: "Mejor calidad" },
};
export const MT = {
  es: "Xenova/opus-mt-es-en", // español a inglés
  en: "Xenova/opus-mt-en-es", // inglés a español
};
export const LANG_NAME = { es: "spanish", en: "english" };

// Limpia marcas de whisper y alucinaciones típicas de silencio.
export function cleanTranscript(raw) {
  let t = (raw || "").replace(/\[[^\]]*\]|\([^)]*\)/g, " ").replace(/\s+/g, " ").trim();
  const junk = ["gracias por ver el video", "thanks for watching", "thank you for watching",
    "subtítulos realizados por la comunidad de amara.org", "you"];
  if (junk.includes(t.toLowerCase().replace(/[^\p{L}\p{N} ]/gu, "").trim())) return "";
  return t;
}
