// Synthesized score + SFX for the Notch film. 120 BPM, drop at 4.0 s, 22.5 s long. Writes score.wav (48 kHz stereo float).
import { writeFileSync } from 'node:fs';

const SR = 48000, DUR = 22.5, N = Math.round(SR * DUR), BEAT = 0.5;
const L = new Float32Array(N), R = new Float32Array(N);
const busL = new Float32Array(N), busR = new Float32Array(N);   // reverb send
let seed = 12345; const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296) * 2 - 1;
const mtof = m => 440 * Math.pow(2, (m - 69) / 12);
const idx = t => Math.round(t * SR);

function add(t0, len, fn, { gain = 1, pan = 0, send = 0 } = {}) {
  const i0 = idx(t0), n = Math.round(len * SR);
  const gl = gain * Math.cos((pan + 1) * Math.PI / 4), gr = gain * Math.sin((pan + 1) * Math.PI / 4);
  for (let i = 0; i < n; i++) {
    const j = i0 + i; if (j < 0 || j >= N) continue;
    const v = fn(i / SR, i);
    L[j] += v * gl; R[j] += v * gr;
    if (send) { busL[j] += v * gl * send; busR[j] += v * gr * send; }
  }
}
// one-pole lowpass factory
const lp1 = () => { let y = 0; return (x, fc) => { const a = 1 - Math.exp(-2 * Math.PI * fc / SR); y += a * (x - y); return y; }; };
const hp1 = () => { const l = lp1(); return (x, fc) => x - l(x, fc); };
// polyBLEP saw
function saw() { let ph = 0; return f => { const dt = f / SR; ph += dt; if (ph >= 1) ph -= 1; let v = 2 * ph - 1;
  if (ph < dt) { const x = ph / dt; v -= x + x - x * x - 1; } else if (ph > 1 - dt) { const x = (ph - 1) / dt; v -= x * x + x + x + 1; } return v; }; }

// ---------- instruments ----------
function kick(t, g = 1) {
  add(t, .55, (u) => { const f = 46 + 95 * Math.exp(-u * 32); const ph = 2 * Math.PI * (46 * u + 95 * (1 - Math.exp(-u * 32)) / 32);
    return Math.sin(ph) * Math.exp(-u * 6.5) * (1 - Math.exp(-u * 900)) + (u < .004 ? rnd() * .35 * (1 - u / .004) : 0); }, { gain: .9 * g });
}
function sub(t, len, midi, g = 1) {
  const f = mtof(midi);
  add(t, len, u => Math.sin(2 * Math.PI * f * u) * Math.min(1, u / .01) * Math.min(1, (len - u) / .05), { gain: .32 * g });
}
function hat(t, g = 1, open = false) {
  const h = hp1(), h2 = hp1();
  add(t, open ? .22 : .06, u => h2(h(rnd(), 7000), 7000) * Math.exp(-u * (open ? 18 : 70)), { gain: .16 * g, pan: .25, send: .05 });
}
function clap(t, g = 1) {
  const h = hp1(), l = lp1();
  add(t, .35, u => { const env = (u < .03 ? (Math.exp(-((u % .01) * 300))) : 1) * Math.exp(-u * 14); return l(h(rnd(), 900), 4500) * env; }, { gain: .28 * g, send: .35 });
}
function pluck(t, midi, g = 1, pan = 0) {
  const f = mtof(midi), l = lp1();
  add(t, .6, u => { const v = Math.sin(2 * Math.PI * f * u) * .7 + Math.sin(2 * Math.PI * f * 2 * u) * .22 * Math.exp(-u * 18) + Math.sin(2 * Math.PI * f * 3.01 * u) * .08 * Math.exp(-u * 30);
    return l(v, 2600) * Math.exp(-u * 9) * Math.min(1, u / .002); }, { gain: .17 * g, pan, send: .3 });
}
function pad(t, len, notes, g, cutoff) {   // detuned saws, cutoff(u) in Hz
  for (const [k, m] of notes.entries()) for (const det of [-.09, .0, .1]) {
    const s = saw(), l = lp1(), l2 = lp1(), f = mtof(m + det);
    add(t, len, u => { const env = Math.min(1, u / .6) * Math.min(1, (len - u) / .8); const c = cutoff(u); return l2(l(s(f), c), c) * env; },
      { gain: g / notes.length * .55, pan: ((k % 2) * 2 - 1) * .35 + det, send: .45 });
  }
}
function bell(t, midi, g = 1, pan = 0) {
  const f = mtof(midi);
  add(t, 2.2, u => (Math.sin(2 * Math.PI * f * u) + .45 * Math.sin(2 * Math.PI * f * 2.76 * u) * Math.exp(-u * 3) + .25 * Math.sin(2 * Math.PI * f * 5.4 * u) * Math.exp(-u * 6))
    * Math.exp(-u * 2.2) * Math.min(1, u / .002), { gain: .12 * g, pan, send: .55 });
}
// UI sounds
function tick(t, g = 1, f = 3200) { const h = hp1(); add(t, .03, u => (h(rnd(), 2000) * .5 + Math.sin(2 * Math.PI * f * u)) * Math.exp(-u * 260), { gain: .2 * g, send: .1 }); }
function morph(t, up = true, g = 1) {   // soft airy "bloop" for the notch changing shape
  const l = lp1(), h = hp1();
  add(t - .06, .45, u => { const f0 = up ? 180 : 420, f1 = up ? 420 : 170; const f = f0 + (f1 - f0) * (1 - Math.exp(-u * 14));
    const tone = Math.sin(2 * Math.PI * f * u) * Math.exp(-u * 11) * Math.min(1, u / .01);
    const air = l(h(rnd(), 900), 3500) * Math.sin(Math.PI * Math.min(1, u / .3)) * .35 * Math.exp(-u * 5);
    return tone * .8 + air; }, { gain: .3 * g, send: .3 });
}
function whoosh(t0, len, g = 1, rise = true) {
  const l = lp1(), h = hp1();
  add(t0, len, u => { const p = u / len, env = rise ? Math.pow(p, 2.2) : Math.pow(1 - p, 2); const fc = rise ? 400 + 7000 * p * p : 6000 - 5000 * p; return l(h(rnd(), 300), fc) * env; }, { gain: .35 * g, send: .4 });
}
function impact(t, g = 1) {
  kick(t, 1.2 * g);
  add(t, 1.6, u => Math.sin(2 * Math.PI * (38 + 30 * Math.exp(-u * 8)) * u) * Math.exp(-u * 2.3), { gain: .42 * g });
  const l = lp1(); add(t, .6, u => l(rnd(), 2400) * Math.exp(-u * 9), { gain: .35 * g, send: .8 });
  add(t, .02, u => rnd() * (1 - u / .02), { gain: .4 * g });
}

// ---------- the score ----------
// D major colour. Chords per 2-s bar: Dmaj9, Bm9, Gmaj9, A6sus
const CH = [[50, 57, 61, 64, 66], [47, 54, 57, 61, 62], [43, 50, 54, 57, 59], [45, 50, 52, 54, 59]];
const ROOT = [38, 35, 31, 33];
const ARP = [[74, 69, 73, 76, 78, 76, 73, 69], [71, 66, 69, 73, 74, 73, 69, 66], [67, 74, 71, 76, 78, 74, 71, 67], [69, 74, 76, 78, 81, 78, 76, 74]];

// intro: eyes in the dark, a held chord that opens as the screen wakes
pad(0, 4.2, [50, 57, 64, 66, 69], .55, u => 220 + 2600 * Math.pow(Math.min(1, Math.max(0, (u - 1.5) / 2.3)), 2));
add(0.33, .35, u => Math.sin(2 * Math.PI * (330 + 260 * (1 - Math.exp(-u * 20))) * u) * Math.exp(-u * 12), { gain: .13, send: .5 });   // eyes open
tick(1.0, .45, 2600); tick(1.36, .45, 2800);   // glances
tick(1.79, .8, 2300);                          // blink
whoosh(1.75, 1.6, .55); whoosh(3.0, 1.0, .8);  // pull-back air, riser into the drop
sub(1.8, 2.2, 38, .5);

// drop + groove 4.0 -> 15.0
impact(4.0, 1);
for (let b = 0; b < 22; b++) {
  const t = 4 + b * BEAT, bar = Math.floor(b / 4) % 4, inBar = b % 4;
  if (t >= 15) break;
  kick(t, inBar === 0 ? 1 : .85);
  sub(t + .02, BEAT * .9, ROOT[bar], 1);
  if (inBar % 2 === 1) clap(t, .8);
  hat(t + BEAT / 2, 1); hat(t + BEAT / 4, .45); hat(t + 3 * BEAT / 4, .45);
  if (b >= 3) for (let s = 0; s < 4; s++) pluck(t + s * BEAT / 4, ARP[bar][(inBar * 4 + s) % 8], s === 0 ? 1 : .7, s % 2 ? .45 : -.45);
}
for (let bar = 0; bar < 6; bar++) {   // ducked pad under the groove
  const t = 4 + bar * 2; if (t >= 15) break;
  pad(t, Math.min(2.1, 15.05 - t), CH[bar % 4].map(m => m + 12), .32, () => 1700);
}
// notch UI sounds
[[5.75, true], [6.5, true], [8.0, false], [9.5, false], [11.0, false], [11.55, true], [12.25, true], [13.0, true], [14.32, false]].forEach(([t, up]) => morph(t, up, .9));
[10.0, 10.5].forEach(t => tick(t, .5, 2000));                        // focus digits roll
for (let k = 0; k < 14; k++) tick(13.32 + k * .052 + (k > 5 ? .09 : 0) + (k > 8 ? .07 : 0) + (k > 11 ? .04 : 0), .3, 3600 + (k % 3) * 300);   // keys
tick(14.18, .8, 1500);                                                 // enter
bell(14.36, 88, .9, -.2); bell(14.5, 95, .7, .2);                      // reminder chime
add(12.24, .2, u => Math.sin(2 * Math.PI * (900 - 500 * u / .2) * u) * Math.exp(-u * 25), { gain: .18, send: .3 });   // shelf drop
whoosh(14.4, .6, .7);

// specs: silence between three heavy hits
[15.0, 16.0, 17.0].forEach((t, i) => { impact(t, 1.05); bell(t + .01, [74, 78, 81][i], .55); });
for (let k = 0; k < 9; k++) tick(16.02 + k * .045 * (1 + k * .12), .35, 2400);   // 38% -> 0% count
whoosh(17.4, .6, .8);

// end card
impact(18.0, .8);
pad(18.0, 4.5, [50, 57, 61, 64, 69, 74], .75, u => 900 + 1400 * Math.min(1, u / 1.5));
sub(18.0, 3.6, 38, .7);
[[18.55, 74], [18.95, 78], [19.35, 81]].forEach(([t, m], i) => bell(t, m, .55, i - 1));
tick(20.18, .7, 2300);                                       // blink
[86, 90, 93].forEach((m, i) => bell(20.82 + i * .06, m, .45, i - 1));   // happy eyes sparkle
tick(21.62, .6, 2100);
impact(21.8, .35);

// ---------- reverb (Schroeder) + sidechain + master ----------
function reverb(inp, combs, aps) {
  const out = new Float32Array(N);
  for (const [d, fb] of combs) { const buf = new Float32Array(d); let p = 0, lpv = 0;
    for (let i = 0; i < N; i++) { const y = buf[p]; lpv = y * .7 + lpv * .3; buf[p] = inp[i] + lpv * fb; p = (p + 1) % d; out[i] += y * .25; } }
  for (const d of aps) { const buf = new Float32Array(d); let p = 0;
    for (let i = 0; i < N; i++) { const b = buf[p], y = -out[i] * .5 + b; buf[p] = out[i] + b * .5; out[i] = y; p = (p + 1) % d; } }
  return out;
}
const rvL = reverb(busL, [[1557, .86], [1617, .86], [1491, .86], [1422, .86]], [225, 556]);
const rvR = reverb(busR, [[1580, .86], [1640, .86], [1514, .86], [1445, .86]], [248, 579]);
let env = 0;
const out = new Float32Array(N * 2);
for (let i = 0; i < N; i++) {
  const t = i / SR;
  // gentle pump on the groove (kick on every beat)
  const inG = t >= 4 && t < 15 ? 1 - .35 * Math.exp(-(((t - 4) % BEAT) * 9)) : 1;
  let l = L[i] + rvL[i] * .9 * inG, r = R[i] + rvR[i] * .9 * inG;
  // soft clip
  l = Math.tanh(l * 1.1) / 1.1; r = Math.tanh(r * 1.1) / 1.1;
  const fade = Math.min(1, (DUR - t) / .06);
  out[2 * i] = l * fade; out[2 * i + 1] = r * fade;
}
// float WAV
const data = Buffer.alloc(out.length * 4); for (let i = 0; i < out.length; i++) data.writeFloatLE(out[i], i * 4);
const hdr = Buffer.alloc(44);
hdr.write('RIFF', 0); hdr.writeUInt32LE(36 + data.length, 4); hdr.write('WAVE', 8); hdr.write('fmt ', 12);
hdr.writeUInt32LE(16, 16); hdr.writeUInt16LE(3, 20); hdr.writeUInt16LE(2, 22); hdr.writeUInt32LE(SR, 24);
hdr.writeUInt32LE(SR * 8, 28); hdr.writeUInt16LE(8, 32); hdr.writeUInt16LE(32, 34); hdr.write('data', 36); hdr.writeUInt32LE(data.length, 40);
writeFileSync(new URL('./score.wav', import.meta.url), Buffer.concat([hdr, data]));
console.log('score.wav written');
