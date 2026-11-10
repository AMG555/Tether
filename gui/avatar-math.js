// Pure animation and placement math for the 3D companion.
//
// Deliberately free of any three.js import so it can be unit-tested in node with no browser,
// no WebGL and no build step (`node --test gui/avatar-math.test.js`). Everything that can get
// the maths wrong lives here; the renderer just applies the numbers.

/** Clamp helper. */
export const clamp = (v, lo, hi) => (v < lo ? lo : v > hi ? hi : v);

/** Linear interpolation. */
export const lerp = (a, b, t) => a + (b - a) * t;

/** Smoothstep — for anything that should not start and stop abruptly. */
export function smoothstep(edge0, edge1, x) {
  const t = clamp((x - edge0) / (edge1 - edge0 || 1e-9), 0, 1);
  return t * t * (3 - 2 * t);
}

/** Frame-rate independent exponential smoothing. `half` = seconds to close half the gap. */
export function damp(current, target, half, dt) {
  if (half <= 0) return target;
  const k = 1 - Math.pow(0.5, dt / half);
  return current + (target - current) * k;
}

/** Gait phase for a walk cycle: two legs out of phase by half a cycle. */
export function gaitPhase(time, tempo, legIndex, legs) {
  const cycles = time * 1.35 * tempo;
  return (cycles + legIndex / legs) % 1;
}

/**
 * Swing angle for a leg at a given gait phase.
 * Front of the cycle swings forward, back swings back, with a lift in the middle of the swing —
 * the reason a procedural walk reads as walking rather than as a pendulum.
 */
export function swingAngle(phase) {
  return Math.sin(phase * Math.PI * 2) * 0.55;
}

/** Vertical foot lift, 0..1, only during the swing half of the gait. */
export function footLift(phase) {
  const swing = phase < 0.5 ? phase : 0;
  return Math.sin((swing / 0.5) * Math.PI) * 0.09;
}

/** Body bob: twice per stride, because a biped's hips dip on each footfall. */
export function bodyBob(time, tempo, amplitude = 0.012) {
  return Math.abs(Math.sin(time * 1.35 * tempo * Math.PI * 2)) * amplitude;
}

/** Breathing: a slow, low-amplitude chest rise. Independent of gait so it survives any state. */
export function breathe(time, rate = 0.22) {
  return Math.sin(time * rate * Math.PI * 2) * 0.5 + 0.5;
}

/**
 * Two-bone IK in a plane. Returns the knee/elbow angle that puts the end effector at `target`
 * distance `d` from the root, given bone lengths `a` and `b`.
 *
 * The law of cosines, with the domain clamped: an unreachable target returns the fully extended
 * (0) or fully folded (π) pose instead of NaN. A NaN rotation silently disappears a limb, which
 * is exactly the kind of bug that ships.
 */
export function twoBoneIK(a, b, d) {
  const reach = a + b;
  const min = Math.abs(a - b) + 1e-6;
  const dd = clamp(d, min, reach - 1e-6);
  const cos = clamp((a * a + b * b - dd * dd) / (2 * a * b), -1, 1);
  return Math.PI - Math.acos(cos);
}

/**
 * Blink schedule: a deterministic-but-organic looking blink from a seed, so two avatars on screen
 * don't blink in lockstep and a recording is reproducible.
 */
export function blinkAmount(time, seed = 0, interval = 4.2) {
  const offset = (seed * 0.618) % 1;
  const t = time / interval + offset;
  const cycle = t % 1;
  const blinkWindow = 0.055; // fraction of the interval spent blinking
  if (cycle > blinkWindow) return 0;
  const p = cycle / blinkWindow;
  return Math.sin(p * Math.PI);
}

/** Eyelid response after a reminder: a wider "attention" blink. */
export function attentiveEye(time, sinceReminder) {
  const flash = Math.exp(-sinceReminder * 2.5) * Math.sin(sinceReminder * 18) * 0.25;
  return clamp(0.5 + flash, 0, 1);
}

/**
 * Root motion for wandering along the desktop. Returns normalised x in [0,1] and facing.
 * The avatar pauses at the edges, because pacing endlessly across a screen is anxiety-inducing.
 */
export function wanderPosition(time, seed, min = 0.05, max = 0.95) {
  const span = max - min;
  const period = 26 + (seed % 7) * 3;
  const t = time / period + (seed * 0.37) % 1;
  // A slow sine with a dwell: hold near the edges rather than bouncing off them.
  const wave = Math.sin(t * Math.PI * 2);
  const eased = Math.sign(wave) * Math.pow(Math.abs(wave), 0.55);
  const x = min + ((eased + 1) / 2) * span;
  const facing = Math.cos(t * Math.PI * 2) >= 0 ? 1 : -1;
  return { x, facing };
}

/** Head look-at, clamped so the avatar doesn't swivel its neck like an owl. */
export function lookAtDelta(avatarX, targetX, maxTurn = 0.6) {
  return clamp((targetX - avatarX) * 2.4, -maxTurn, maxTurn);
}

/** Keep a speech bubble inside the viewport, flipping above/below the head if needed. */
export function placeBubble(headX, headY, bubbleW, bubbleH, viewW, viewH, gap = 10) {
  let x = headX - bubbleW / 2;
  let y = headY - bubbleH - gap;
  let flipped = false;
  if (y < 4) {
    y = headY + gap;
    flipped = true;
  }
  x = clamp(x, 4, Math.max(4, viewW - bubbleW - 4));
  y = clamp(y, 4, Math.max(4, viewH - bubbleH - 4));
  return { x, y, flipped };
}

/** Body pose amplitude for each companion state, so states cross-fade instead of snapping. */
export const POSE_WEIGHTS = {
  idle:      { breathe: 0.35, sway: 0.25, swing: 0.0,  sit: 0.0, arm: 0.05, tail: 0.35, eyes: 1.0 },
  walk:      { breathe: 0.30, sway: 0.35, swing: 1.0,  sit: 0.0, arm: 0.55, tail: 0.45, eyes: 1.0 },
  sit:       { breathe: 0.25, sway: 0.10, swing: 0.0,  sit: 1.0, arm: 0.10, tail: 0.55, eyes: 1.0 },
  wave:      { breathe: 0.30, sway: 0.15, swing: 0.0,  sit: 0.0, arm: 1.0,  tail: 0.35, eyes: 1.0 },
  celebrate: { breathe: 0.40, sway: 0.30, swing: 0.0,  sit: 0.0, arm: 0.9,  tail: 1.0,  eyes: 1.0 },
  sleep:     { breathe: 0.55, sway: 0.05, swing: 0.0,  sit: 0.85, arm: 0.0, tail: 0.08, eyes: 0.0 },
  pet:       { breathe: 0.3, sway: 0.1, swing: 0.0,  sit: 0.7, arm: 0.15, tail: 1.0,  eyes: 0.05 },
};

/** Blend two pose-weight sets by `t`. Used for every state transition. */
export function blendPose(from, to, t) {
  const out = {};
  for (const key of Object.keys(POSE_WEIGHTS.idle)) {
    out[key] = lerp(from?.[key] ?? 0, to?.[key] ?? 0, t);
  }
  return out;
}

/**
 * Picks the state the companion should be in, from coarse facts only.
 * Note what is *not* an input: typing speed, word count, focus duration, app name.
 */
export function chooseState({ reminderActive, celebrating, sleeping, walking, sincePet, dragged }) {
  if (dragged) return 'pet';
  if (celebrating) return 'celebrate';
  if (reminderActive) return 'wave';
  if (sleeping) return 'sleep';
  if (walking) return 'walk';
  if (sincePet < 1.5) return 'pet';
  return 'idle';
}

/** Convert "#rrggbb" to [r,g,b] in 0..1, falling back instead of producing NaN colours. */
export function hexToRgb01(hex, fallback = [0.55, 0.36, 0.96]) {
  if (typeof hex !== 'string') return fallback;
  const s = hex.trim().replace('#', '');
  if (!/^[0-9a-fA-F]{6}$/.test(s)) return fallback;
  return [
    parseInt(s.slice(0, 2), 16) / 255,
    parseInt(s.slice(2, 4), 16) / 255,
    parseInt(s.slice(4, 6), 16) / 255,
  ];
}

/** Should the framebuffer be transparent (desktop pet) or show a studio backdrop (preview)? */
export function shouldClearAlpha(desktopMode) {
  return desktopMode ? 0 : 1;
}
