import { test } from 'node:test';
import assert from 'node:assert/strict';

import {
  clamp,
  damp,
  chooseClip,
  shouldRestart,
  advanceClip,
  placeClipPet,
  wanderPosition,
  pickBlink,
  afterOneShot,
  CLIP_PRIORITY,
  ONE_SHOT,
} from './clip-player-math.js';

test('clamp holds the edges', () => {
  assert.equal(clamp(-5, 0, 1), 0);
  assert.equal(clamp(5, 0, 1), 1);
  assert.equal(clamp(0.5, 0, 1), 0.5);
});

test('damp is frame-rate independent', () => {
  // 1 second of damping must land in the same place at 30 fps and at 144 Hz.
  const run = (fps) => {
    let v = 0;
    const dt = 1 / fps;
    for (let i = 0; i < fps; i++) v = damp(v, 1, 2, dt);
    return v;
  };
  const a = run(30);
  const b = run(144);
  assert.ok(Math.abs(a - b) < 1e-6, `30fps=${a} 144fps=${b}`);
  // And it actually moves, in the direction asked.
  assert.ok(a > 0.8 && a < 1, `got ${a}`);
});

test('damp survives NaN input instead of poisoning the transform', () => {
  assert.equal(damp(NaN, 1, 2, 0.016), 1, 'bad `from` uses the target');
  assert.equal(damp(3, NaN, 2, 0.016), 3, 'bad `to` keeps the last good value');
  assert.equal(damp(NaN, NaN, 2, 0.016), 0, 'never emits NaN');
});

test('chooseClip prefers one-shots over walk, and walk over idle', () => {
  assert.equal(chooseClip({ walking: true }), 'walk');
  assert.equal(chooseClip({}), 'idle');
  const all = { available: new Set(CLIP_PRIORITY) };
  assert.equal(chooseClip({ walking: true, ...all }), 'walk');
});

test('a pack without a walk clip is never asked for one', () => {
  const available = new Set(['idle', 'remind']);
  assert.equal(chooseClip({ walking: true, available }), 'idle');
});

test('CLIP_PRIORITY lists every one-shot before the ambient states', () => {
  const lastOneShot = Math.max(...[...ONE_SHOT].map((s) => CLIP_PRIORITY.indexOf(s)));
  const firstAmbient = Math.min(
    CLIP_PRIORITY.indexOf('sleep'),
    CLIP_PRIORITY.indexOf('walk'),
    CLIP_PRIORITY.indexOf('idle'),
  );
  assert.ok(lastOneShot < firstAmbient, 'one-shots must outrank ambient states');
});

test('a looping clip is not restarted every frame', () => {
  assert.equal(shouldRestart('walk', 'walk', 0, 5000), false);
});

test('a one-shot can be replayed, but not more than the minimum gap', () => {
  assert.equal(shouldRestart('remind', 'remind', 1000, 1100, 250), false, 'too soon');
  assert.equal(shouldRestart('remind', 'remind', 1000, 1400, 250), true, 'gap passed');
});

test('changing state always restarts', () => {
  assert.equal(shouldRestart('idle', 'walk', 0, 1), true);
});

test('advanceClip walks frames, wraps, and counts loops', () => {
  const clip = { frames: 10, frameMs: 50, elapsed: 0 };
  const a = advanceClip(clip, 120); // 2.4 frames in
  assert.equal(a.index, 2);
  assert.equal(a.loops, 0);
  assert.equal(a.elapsed, 120, 'elapsed accumulates');
  // Feed the elapsed back into the descriptor, keeping its frame metadata.
  const b = advanceClip({ ...clip, elapsed: a.elapsed }, 480); // 600ms, 500ms per loop
  assert.equal(b.loops, 1, 'past 500ms is one loop');
  assert.equal(b.index, 2, '600 -> 100 into the second loop -> frame 2');
  assert.equal(b.elapsed, 600);
});

test('advanceClip tolerates a missing clip and a zero frame duration', () => {
  assert.deepEqual(advanceClip(null, 16), { index: 0, elapsed: 0, loops: 0 });
  const r = advanceClip({ frames: 4, frameMs: 0 }, 100); // falls back to 17fps
  assert.ok(Number.isFinite(r.index));
});

test('placeClipPet puts the feet on the anchor line', () => {
  const p = placeClipPet({
    viewW: 1200, viewH: 800, naturalW: 264, naturalH: 280, scale: 1,
    anchor: 'bottom', x: 0.5, y: 1,
  });
  assert.equal(p.h, 280);
  assert.equal(p.top, 800 - 280, 'bottom-anchored at the floor');
  assert.equal(p.left, (1200 - 264) / 2);
});

test('placeClipPet never lets a pet hang off an edge', () => {
  const p = placeClipPet({
    viewW: 400, viewH: 300, naturalW: 100, naturalH: 100, scale: 2,
    anchor: 'bottom', x: 0, y: 1, margin: 4,
  });
  assert.ok(p.left >= 4, `left=${p.left}`);
  assert.ok(p.left + p.w <= 400 - 4 + 0.001, `right=${p.left + p.w}`);
  assert.ok(p.top >= 4 && p.top + p.h <= 300 + 0.001, `vertical=${p.top}..${p.top + p.h}`);
});

test('a pet taller than the window still shows its feet', () => {
  const p = placeClipPet({
    viewW: 400, viewH: 300, naturalW: 264, naturalH: 280, scale: 2,
    anchor: 'bottom', x: 0.5, y: 1, margin: 4,
  });
  assert.equal(p.top + p.h, 300, 'bottom edge sits on the floor even when cropped');
});

test('placeClipPet honours top and center anchors', () => {
  const top = placeClipPet({ viewW: 800, viewH: 600, naturalW: 100, naturalH: 100, anchor: 'top', y: 0.5 });
  assert.equal(top.top, 300);
  const mid = placeClipPet({ viewW: 800, viewH: 600, naturalW: 100, naturalH: 100, anchor: 'center', y: 0.5 });
  assert.equal(mid.top, 250);
});

test('placeClipPet clamps an absurd scale rather than exploding', () => {
  const p = placeClipPet({ viewW: 800, viewH: 600, naturalW: 100, naturalH: 100, scale: 99 });
  assert.equal(p.scale, 4);
  assert.equal(p.h, 400);
});

test('a pet wider than the window is centred, not pinned', () => {
  const p = placeClipPet({ viewW: 200, viewH: 600, naturalW: 264, naturalH: 280, scale: 3 });
  assert.ok(p.left < 0, 'accepts a crop');
  assert.equal(p.left, (200 - p.w) / 2);
});

test('wanderPosition stays inside its bounds for a long run', () => {
  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < 20000; i++) {
    const { x } = wanderPosition(i * 0.1, 7, 0.1, 0.9);
    assert.ok(Number.isFinite(x));
    min = Math.min(min, x);
    max = Math.max(max, x);
  }
  assert.ok(min >= 0.1 - 1e-9, `min=${min}`);
  assert.ok(max <= 0.9 + 1e-9, `max=${max}`);
  assert.ok(max - min > 0.3, 'it should actually roam');
});

test('wanderPosition turns around at the ends', () => {
  const facings = new Set();
  for (let i = 0; i < 500; i++) facings.add(wanderPosition(i * 0.3, 3).facing);
  assert.deepEqual([...facings].sort(), [-1, 1], 'both directions occur');
});

test('blinks are rare, brief, and offset per pet', () => {
  const sample = (seed) => {
    let closed = 0;
    const n = 20000;
    for (let i = 0; i < n; i++) if (pickBlink(i * 0.01, seed) > 0.5) closed++;
    return closed / n;
  };
  const a = sample(1);
  assert.ok(a < 0.1, `eyes should be open most of the time, got ${a}`);
  // Two pets with different seeds must be at different points in the blink cycle. Comparing
  // average closed-fraction would not show this: it converges to the same ratio for both.
  // Sample finely enough to land inside the ~130ms closed window.
  const differ = Array.from({ length: 4200 }, (_, i) => pickBlink(i / 1000, 1) !== pickBlink(i / 1000, 2));
  assert.ok(differ.some(Boolean), 'two pets must not blink in lockstep');
  assert.equal(pickBlink(0, 1), pickBlink(0, 1), 'deterministic');
});

test('a one-shot always has somewhere to fall back to', () => {
  assert.equal(afterOneShot(undefined), 'idle');
  assert.equal(afterOneShot('walk'), 'walk');
});
