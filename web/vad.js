// Detector de frases por energía (misma lógica que core/src/vad.rs).
const RATE = 16000, BLOCK = 320;

export class Resampler {
  constructor(from, to = RATE) { this.ratio = from / to; this.pos = 0; this.buf = []; }
  push(input) {
    for (let i = 0; i < input.length; i++) this.buf.push(input[i]);
    const out = [];
    for (;;) {
      const end = this.pos + this.ratio, e = Math.ceil(end);
      if (e >= this.buf.length) break;
      const s = Math.floor(this.pos);
      let sum = 0, n = 0;
      for (let i = s; i < Math.max(e, s + 1); i++) { sum += this.buf[i]; n++; }
      out.push(sum / n);
      this.pos = end;
    }
    const used = Math.floor(this.pos);
    if (used > 0) { this.buf.splice(0, used); this.pos -= used; }
    return out;
  }
}

export class Segmenter {
  constructor({ threshold = 0.012, silence = 0.7, min = 0.5, max = 12 } = {}) {
    Object.assign(this, { threshold, silence, min, max });
    this.buf = []; this.pending = []; this.inSpeech = false; this.silent = 0;
  }
  push(samples) {
    const out = [];
    for (let i = 0; i < samples.length; i++) this.pending.push(samples[i]);
    while (this.pending.length >= BLOCK) {
      const block = this.pending.splice(0, BLOCK);
      let s = 0; for (const v of block) s += v * v;
      const voiced = Math.sqrt(s / BLOCK) >= this.threshold;
      if (voiced) { this.inSpeech = true; this.silent = 0; this.buf.push(...block); }
      else if (this.inSpeech) {
        this.silent += BLOCK; this.buf.push(...block);
        if (this.silent / RATE >= this.silence) this._finish(out);
      }
      if (this.inSpeech && this.buf.length / RATE >= this.max) this._finish(out);
    }
    return out;
  }
  flush() { const o = []; this._finish(o); return o[0]; }
  _finish(out) {
    const speech = (this.buf.length - this.silent) / RATE;
    if (speech >= this.min) out.push(Float32Array.from(this.buf));
    this.buf = []; this.inSpeech = false; this.silent = 0;
  }
}
