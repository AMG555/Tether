// node --test gui/avatar-math.test.js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  clamp, lerp, smoothstep, damp, gaitPhase, swingAngle, footLift, bodyBob, breathe,
  twoBoneIK, blinkAmount, attentiveEye, wanderPosition, lookAtDelta, placeBubble,
  POSE_WEIGHTS, blendPose, chooseState, hexToRgb01, shouldClearAlpha,
} from './avatar-math.js';

test('clamp and lerp behave at the boundaries', () => {
  assert.equal(clamp(-5, 0, 1), 0);
  assert.equal(clamp(5, 0, 1), 1);
  assert.equal(clamp(0.5, 0, 1), 0.5);
  assert.equal(lerp(0, 10, 0), 0);
  assert.equal(lerp(0, 10, 1), 10);
  assert.equal(lerp(0, 10, 0.5), 5);
});

test('smoothstep is monotonic, bounded and flat at the ends', () => {
  assert.equal(smoothstep(0, 1, -1), 0);
  assert.equal(smoothstep(0, 1, 2), 1);
  let prev = -1;
  for (let i = 0; i <= 20; i++) {
    const v = smoothstep(0, 1, i / 20);
    assert.ok(v >= prev, 'monotonic');
    assert.ok(v >= 0 && v <= 1);
    prev = v;
  }
  assert.ok(smoothstep(0, 1, 0.05) < 0.05, 'eases in');
  assert.ok(smoothstep(0, 1, 0.95) > 0.95, 'eases out');
});

test('smoothstep survives a zero-width range instead of dividing by zero', () => {
  assert.ok(Number.isFinite(smoothstep(1, 1, 1)));
});

test('damp converges and is frame-rate independent', () => {
  // Reaching the same value in one 1s step vs sixty 1/60s steps.
  const one = damp(0, 1, 0.2, 1.0);
  let many = 0;
  for (let i = 0; i < 60; i++) many = damp(many, 1, 0.2, 1 / 60);
  assert.ok(Math.abs(one - many) < 1e-6, `${one} vs ${many}`);
  assert.ok(one > 0.9 && one < 1, 'closed most of the gap in one half-life x5');
  assert.equal(damp(0, 1, 0, 0.016), 1, 'zero half-life snaps');
});

test('gait phases are evenly distributed and wrap', () => {
  const legs = 4;
  for (let i = 0; i < legs; i++) {
    const p = gaitPhase(0, 1, i, legs);
    assert.ok(p >= 0 && p < 1);
  }
  // At t=0 the four feet are at 0, .25, .5, .75 — evenly spread, no two in step.
  const phases = [0, 1, 2, 3].map((i) => gaitPhase(0, 1, i, 4));
  assert.deepEqual(phases.map((p) => Math.round(p * 100) / 100), [0, 0.25, 0.5, 0.75]);
  // And it wraps rather than growing forever.
  assert.ok(gaitPhase(1000, 2, 1, 2) < 1);
});

test('a swing cycle is symmetric and bounded', () => {
  for (let i = 0; i <= 40; i++) {
    const a = swingAngle(i / 40);
    assert.ok(Math.abs(a) <= 0.56, 'bounded swing');
  }
  assert.ok(Math.abs(swingAngle(0.25) + swingAngle(0.75)) < 1e-9, 'symmetric');
});

test('feet only lift during the swing half of a gait', () => {
  assert.equal(footLift(0), 0);
  assert.equal(footLift(0.75), 0, 'stance foot stays planted');
  assert.ok(footLift(0.25) > 0.08, 'swing foot lifts');
});

test('body bob peaks twice per stride', () => {
  const samples = [];
  for (let i = 0; i < 100; i++) samples.push(bodyBob(i / 20, 1, 1));
  const peaks = samples.filter((v, i) => i > 0 && i < samples.length - 1 && v > samples[i - 1] && v > samples[i + 1]);
  assert.ok(peaks.length >= 2, `expected at least 2 peaks, got ${peaks.length}`);
});

test('breathing stays in 0..1 and never stalls', () => {
  const vals = [];
  for (let i = 0; i < 50; i++) vals.push(breathe(i / 10));
  assert.ok(vals.every((v) => v >= 0 && v <= 1));
  assert.ok(Math.max(...vals) - Math.min(...vals) > 0.5, 'it actually breathes');
});

test('two-bone IK is finite everywhere, including unreachable targets', () => {
  const a = 0.4, b = 0.4;
  for (const d of [-10, 0, 0.001, 0.1, 0.4, 0.79, 0.8, 5, 1000]) {
    const angle = twoBoneIK(a, b, d);
    assert.ok(Number.isFinite(angle), `d=${d} produced ${angle}`);
    assert.ok(angle >= 0 && angle <= Math.PI, `d=${d} out of range: ${angle}`);
  }
});

test('two-bone IK folds more as the target gets closer', () => {
  const near = twoBoneIK(0.4, 0.4, 0.2);
  const far = twoBoneIK(0.4, 0.4, 0.75);
  assert.ok(near > far, `near=${near} should bend more than far=${far}`);
});

test('IK handles unequal bone lengths without NaN', () => {
  for (const [a, b] of [[0.5, 0.1], [0.1, 0.5], [0.5, 0.5]]) {
    for (const d of [0, 0.2, 0.6, 5]) {
      assert.ok(Number.isFinite(twoBoneIK(a, b, d)));
    }
  }
});

test('blinks are brief, bounded and offset per avatar', () => {
  let open = 0, closed = 0;
  for (let i = 0; i < 600; i++) {
    const v = blinkAmount(i / 30, 0);
    assert.ok(v >= 0 && v <= 1);
    if (v > 0.5) closed++;
    if (v === 0) open++;
  }
  assert.ok(open > closed * 5, 'eyes are open most of the time');
  // Two avatars must not blink in lockstep.
  const a = Array.from({ length: 200 }, (_, i) => blinkAmount(i / 30, 0));
  const b = Array.from({ length: 200 }, (_, i) => blinkAmount(i / 30, 1));
  assert.notDeepEqual(a, b);
});

test('attention eye stays in range and settles', () => {
  for (const t of [0, 0.01, 0.1, 0.5, 2]) {
    const v = attentiveEye(t, t);
    assert.ok(v >= 0 && v <= 1, `t=${t} -> ${v}`);
  }
  assert.ok(Math.abs(attentiveEye(5, 5) - 0.5) < 0.05, 'settles back to normal');
});

test('wandering stays inside bounds and turns around', () => {
  let min = 1, max = 0;
  const facings = new Set();
  for (let i = 0; i < 2000; i++) {
    const { x, facing } = wanderPosition(i / 10, 3, 0.1, 0.9);
    assert.ok(x >= 0.1 - 1e-9 && x <= 0.9 + 1e-9, `x=${x} escaped bounds`);
    min = Math.min(min, x);
    max = Math.max(max, x);
    facings.add(facing);
  }
  assert.ok(max - min > 0.4, 'actually wanders');
  assert.equal(facings.size, 2, 'and changes direction');
});

test('head turn is clamped', () => {
  assert.equal(lookAtDelta(0.5, 0.5), 0);
  assert.ok(Math.abs(lookAtDelta(0, 100)) <= 0.6);
  assert.ok(lookAtDelta(0.4, 0.6) > 0);
});

test('speech bubble is clamped inside the viewport and flips when it would clip', () => {
  const a = placeBubble(400, 300, 160, 60, 800, 600);
  assert.ok(a.x >= 4 && a.x + 160 <= 796);
  assert.ok(a.y >= 4 && a.y + 60 <= 596);

  // Near the left edge: must not go negative.
  const left = placeBubble(10, 300, 160, 60, 800, 600);
  assert.ok(left.x >= 4, `x=${left.x}`);

  // Near the top: must flip below the head rather than clip.
  const top = placeBubble(400, 30, 160, 60, 800, 600);
  assert.equal(top.flipped, true);
  assert.ok(top.y > 30, 'flipped below the head');

  // Tiny viewport must still return something on screen.
  const tiny = placeBubble(5, 5, 300, 200, 320, 240);
  assert.ok(tiny.x >= 4 && tiny.y >= 4);
});

test('the renderer can only ever read finite pose values', () => {
  // Regression for two separate bugs where a renamed key (p.arm0, c.height) went undefined,
  // turned a rotation into NaN, and made a body part vanish with no error at all.
  const required = ['breathe', 'sway', 'swing', 'sit', 'arm', 'tail', 'eyes'];
  for (const key of required) {
    assert.ok(key in POSE_WEIGHTS.idle, `POSE_WEIGHTS is missing "${key}"`);
    for (const state of Object.keys(POSE_WEIGHTS)) {
      assert.equal(typeof POSE_WEIGHTS[state][key], 'number', `${state}.${key}`);
      assert.ok(Number.isFinite(POSE_WEIGHTS[state][key]), `${state}.${key} not finite`);
    }
  }
});

test('every state has a complete weight set', () => {
  const keys = Object.keys(POSE_WEIGHTS.idle).sort();
  for (const [name, w] of Object.entries(POSE_WEIGHTS)) {
    assert.deepEqual(Object.keys(w).sort(), keys, `${name} is missing channels`);
    for (const [k, v] of Object.entries(w)) {
      assert.ok(v >= 0 && v <= 1, `${name}.${k} = ${v}`);
    }
  }
});

test('blending two poses lands exactly on both ends', () => {
  const a = blendPose(POSE_WEIGHTS.idle, POSE_WEIGHTS.walk, 0);
  assert.equal(a.swing, POSE_WEIGHTS.idle.swing);
  const b = blendPose(POSE_WEIGHTS.idle, POSE_WEIGHTS.walk, 1);
  assert.equal(b.swing, POSE_WEIGHTS.walk.swing);
  const mid = blendPose(POSE_WEIGHTS.idle, POSE_WEIGHTS.walk, 0.5);
  assert.ok(mid.swing > 0 && mid.swing < 1);
});

test('blend tolerates missing weight sets', () => {
  const out = blendPose(undefined, POSE_WEIGHTS.walk, 1);
  assert.equal(out.swing, POSE_WEIGHTS.walk.swing);
});

test('state machine prefers the most urgent state', () => {
  const base = { reminderActive: false, celebrating: false, sleeping: false, walking: false, sincePet: 99, dragged: false };
  assert.equal(chooseState(base), 'idle');
  assert.equal(chooseState({ ...base, walking: true }), 'walk');
  assert.equal(chooseState({ ...base, reminderActive: true }), 'wave');
  assert.equal(chooseState({ ...base, reminderActive: true, celebrating: true }), 'celebrate');
  assert.equal(chooseState({ ...base, dragged: true }), 'pet');
  assert.equal(chooseState({ ...base, sleeping: true }), 'sleep');
  assert.equal(chooseState({ ...base, sincePet: 0.5 }), 'pet');
});

test('colour parsing never produces NaN', () => {
  assert.deepEqual(hexToRgb01('#ff0000'), [1, 0, 0]);
  assert.deepEqual(hexToRgb01('00ff00'), [0, 1, 0]);
  for (const bad of ['', 'nope', '#12', '#gggggg', null, undefined, 42]) {
    assert.deepEqual(hexToRgb01(bad), [0.55, 0.36, 0.96], `bad input ${bad}`);
  }
});

test('desktop mode clears alpha to zero, preview does not', () => {
  assert.equal(shouldClearAlpha(true), 0);
  assert.equal(shouldClearAlpha(false), 1);
});
