// node render.mjs stills 1.0 3.0 ...   -> stills/t_<sec>.png
// node render.mjs film                  -> notch-video.mp4 (silent, 60 fps)
import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const dir = path.dirname(fileURLToPath(import.meta.url));
const [mode, ...rest] = process.argv.slice(2);
const browser = await chromium.launch({ channel: 'msedge', args: ['--force-color-profile=srgb', '--disable-lcd-text'] });
const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 1 });
page.on('pageerror', e => { console.error('PAGE ERROR', e.message); process.exit(1); });
await page.goto(pathToFileURL(path.join(dir, 'film.html')).href);
await page.evaluate(() => window.ready);
const canvas = page.locator('#c');

if (mode === 'stills') {
  mkdirSync(path.join(dir, 'stills'), { recursive: true });
  for (const s of rest) {
    await page.evaluate(f => renderFrame(f), Math.round(+s * 60));
    await canvas.screenshot({ path: path.join(dir, 'stills', `t_${s}.png`) });
  }
} else {
  const frames = await page.evaluate(() => window.FRAMES);
  const out = path.join(dir, 'notch-video.mp4');
  const ff = spawn('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'image2pipe', '-framerate', '60', '-i', '-',
    '-c:v', 'libx264', '-preset', 'slow', '-crf', '14', '-pix_fmt', 'yuv420p', '-tune', 'film',
    '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'bt709', '-movflags', '+faststart', out], { stdio: ['pipe', 'inherit', 'inherit'] });
  const t0 = Date.now();
  for (let i = 0; i < frames; i++) {
    await page.evaluate(f => renderFrame(f), i);
    const buf = await canvas.screenshot({ type: 'png' });
    if (!ff.stdin.write(buf)) await new Promise(r => ff.stdin.once('drain', r));
    if (i % 60 === 0) console.log(`frame ${i}/${frames}  ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  }
  ff.stdin.end();
  await new Promise(r => ff.on('close', r));
  console.log('wrote', out);
}
await browser.close();
