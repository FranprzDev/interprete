// Test de Node: mismos modelos y opciones que la web, con audio real (espeak) -> whisper -> traducción.
import { readFileSync } from "node:fs";
import { pipeline } from "@huggingface/transformers";
import { STT, MT, LANG_NAME, cleanTranscript } from "../config.js";

function readWav(path) {
  const b = readFileSync(path);
  const rate = b.readUInt32LE(24), ch = b.readUInt16LE(22);
  let off = 12;
  while (b.toString("ascii", off, off + 4) !== "data") off += 8 + b.readUInt32LE(off + 4);
  const n = b.readUInt32LE(off + 4) / 2 / ch, out = new Float32Array(Math.floor(n * 16000 / rate));
  for (let i = 0; i < out.length; i++) out[i] = b.readInt16LE(off + 8 + Math.floor(i * rate / 16000) * 2 * ch) / 32768;
  return out;
}

const which = process.env.STT || "base";
const asr = await pipeline("automatic-speech-recognition", STT[which].id, { dtype: "q8" });
const mt = { es: await pipeline("translation", MT.es, { dtype: "q8" }), en: await pipeline("translation", MT.en, { dtype: "q8" }) };
const cases = [
  ["es", "wav/es1.wav", /solar/i], ["es", "wav/es2.wav", /meeting|professor|teacher/i],
  ["en", "wav/en1.wav", /solar/i], ["en", "wav/en2.wav", /reuni[óo]n|profesor/i],
];
let fail = 0;
for (const [lang, file, expect] of cases) {
  const audio = readWav(file);
  const t0 = performance.now();
  const text = cleanTranscript((await asr(audio, { language: LANG_NAME[lang], task: "transcribe" })).text);
  const t1 = performance.now();
  const tr = (await mt[lang](text, { max_new_tokens: 200 }))[0].translation_text;
  console.log(`${file}\n  ORIGINAL:   ${text}\n  TRADUCCION: ${tr}\n  (voz ${Math.round(t1 - t0)} ms, traduccion ${Math.round(performance.now() - t1)} ms)`);
  if (!expect.test(tr)) { console.log("  FALLA: no contiene lo esperado"); fail++; }
}
process.exit(fail ? 1 : 0);
