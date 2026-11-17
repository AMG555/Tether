import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const desktopRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

test('companion-overlay boots cleanly when performance.now() < 200ms and starts animation', async (t) => {
  const rafCallbacks = [];
  const intervals = [];
  const timeouts = [];

  const createEl = () => ({
    style: {},
    classList: { add() {}, remove() {}, toggle() {} },
    setAttribute() {},
    appendChild() {},
    removeChild() {},
    addEventListener() {},
    getContext() {
      return {
        scale() {},
        clearRect() {},
        save() {},
        restore() {},
        beginPath() {},
        roundRect() {},
        fill() {},
        stroke() {},
        moveTo() {},
        lineTo() {},
        closePath() {},
        arc() {},
        ellipse() {},
        translate() {},
        rotate() {},
        fillText() {},
        measureText: (txt) => ({ width: String(txt || '').length * 7 }),
      };
    },
  });

  const canvas = createEl();
  const sprite = createEl();
  const hitZone = createEl();
  const body = createEl();

  const elements = new Map([
    ['companion-canvas', canvas],
    ['companion-sprite', sprite],
    ['hit-zone', hitZone],
  ]);

  const saved = {
    window: globalThis.window,
    document: globalThis.document,
    performance: globalThis.performance,
    requestAnimationFrame: globalThis.requestAnimationFrame,
    cancelAnimationFrame: globalThis.cancelAnimationFrame,
    Image: globalThis.Image,
    setInterval: globalThis.setInterval,
    setTimeout: globalThis.setTimeout,
  };

  t.after(() => {
    for (const id of intervals) clearInterval(id);
    for (const id of timeouts) clearTimeout(id);
    Object.assign(globalThis, saved);
  });

  globalThis.window = {
    innerWidth: 360,
    innerHeight: 310,
    devicePixelRatio: 1,
    screen: { width: 1920, height: 1080 },
    addEventListener() {},
  };
  globalThis.document = {
    hidden: false,
    body,
    getElementById: (id) => elements.get(id) ?? null,
    createElement: () => createEl(),
    addEventListener() {},
  };
  // Simulate realistic WebView2 cold boot: performance.now() is < 200 ms.
  globalThis.performance = { now: () => 25 };
  globalThis.requestAnimationFrame = (cb) => {
    rafCallbacks.push(cb);
    return rafCallbacks.length;
  };
  globalThis.cancelAnimationFrame = () => {};
  globalThis.Image = class MockImage {
    constructor() {
      this.src = '';
      this.naturalWidth = 0;
      this.naturalHeight = 0;
    }
  };
  globalThis.setInterval = (fn, ms) => {
    const id = saved.setInterval(fn, ms);
    intervals.push(id);
    return id;
  };
  globalThis.setTimeout = (fn, ms) => {
    const id = saved.setTimeout(fn, ms);
    timeouts.push(id);
    return id;
  };

  // Must not throw ReferenceError (e.g. Temporal Dead Zone on _dragActive)
  await import('../src/companion-overlay.js');

  // Render loop must have started and hit-zone must be positioned
  assert.ok(rafCallbacks.length >= 1, 'requestAnimationFrame must be scheduled on boot');
  assert.ok(hitZone.style.left, 'hitZone.style.left must be set on boot');
  assert.ok(hitZone.style.top, 'hitZone.style.top must be set on boot');
  assert.ok(hitZone.style.width, 'hitZone.style.width must be set on boot');
  assert.ok(hitZone.style.height, 'hitZone.style.height must be set on boot');

  // Execute one animation frame to ensure renderFrame runs without throwing
  rafCallbacks[0](42);
});

test('companion-overlay canvas stacks above sprite so captions never render behind head', async () => {
  const html = await readFile(path.join(desktopRoot, 'src/companion-overlay.html'), 'utf8');
  const spriteBlock = html.match(/#companion-sprite\s*\{[^}]*\}/);
  const canvasBlock = html.match(/canvas\s*\{[^}]*\}/);
  assert.ok(spriteBlock && canvasBlock, 'both #companion-sprite and canvas rules must exist');

  const spriteZ = Number(spriteBlock[0].match(/z-index:\s*(\d+)/)?.[1]);
  const canvasZ = Number(canvasBlock[0].match(/z-index:\s*(\d+)/)?.[1]);
  assert.ok(canvasZ > spriteZ, `canvas z-index (${canvasZ}) must be greater than #companion-sprite (${spriteZ})`);
});

test('companion-overlay window stays compact to prevent browser occlusion and white flashing', async () => {
  const cfg = JSON.parse(await readFile(path.join(desktopRoot, 'src-tauri/tauri.conf.json'), 'utf8'));
  const overlay = cfg.app.windows.find((w) => w.label === 'companion-overlay');
  assert.ok(overlay, 'companion-overlay window must be configured');
  assert.ok(overlay.width <= 480 && overlay.height <= 400,
    `overlay must be compact (got ${overlay.width}x${overlay.height}) so Chromium occlusion never blanks browsers`);
});
