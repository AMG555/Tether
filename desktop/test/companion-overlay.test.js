import assert from 'node:assert/strict';
import test from 'node:test';

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
    for (const id of intervals) saved.setInterval && clearInterval(id);
    for (const id of timeouts) clearTimeout(id);
    Object.assign(globalThis, saved);
  });

  globalThis.window = {
    innerWidth: 1920,
    innerHeight: 1076,
    devicePixelRatio: 1,
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
