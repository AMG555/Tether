/**
 * Deterministic math for the clip-pet player. No DOM, no timers, no dependencies.
 *
 * Everything here takes `now`/`dt` as arguments rather than reading a clock, which is what makes
 * desktop behaviour testable: a window-clamping bug or a wander path that walks off screen can be
 * caught by `node --test` instead of by watching a pet for an hour.
 *
 * Verified by `clip-player-math.test.js`.
 */

/** States in priority order: the first one available and active wins. */
export const CLIP_PRIORITY = [
  'celebrate',
  'remind',
  'water',
  'stretch',
  'eyes',
  'happy',
  'sad',
  'sleep',
  'walk',
  'idle',
];

/** States that play once and then return to whatever was happening. */
export const ONE_SHOT = new Set(['celebrate', 'remind', 'water', 'stretch', 'eyes', 'happy', 'sad']);

export function clamp(v, lo, hi) {
  return v < lo ? lo : v > hi ? hi : v;
}

export function lerp(a, b, t) {
  return a + (b - a) * t;
}

/**
 * Frame-rate independent exponential smoothing.
 *
 * `damp(from, to, rate, dt)`: the fraction remaining after `dt` seconds is `exp(-rate·dt)`, so a
 * 30 fps machine and a 144 Hz machine reach the same place at the same wall-clock time. Naive
 * `a + (b-a)*0.1` per frame does not, and a pet that walks faster on a gaming monitor is a bug
 * users notice.
 */
export function damp(from, to, rate, dt) {
  // Never let a bad input poison a transform: a NaN scale or position makes an element
  // invisible in the DOM *and* is impossible to debug from a screenshot.
  if (!Number.isFinite(to)) return Number.isFinite(from) ? from : 0;
  if (!Number.isFinite(from)) return to;
  const t = 1 - Math.exp(-Math.max(0, rate) * Math.max(0, dt));
  return from + (to - from) * t;
}

export function smoothstep(t) {
  const x = clamp(t, 0, 1);
  return x * x * (3 - 2 * x);
}

/**
 * Picks the clip to display given behavioural flags.
 *
 * Returns the highest-priority active state that the pack actually has. A pack without `walk`
 * will never be asked for a walk: the caller passes availability via `available`.
 */
export function chooseClip({ walking = false, sleeping = false, available = null } = {}) {
  const active = new Set();
  if (walking) active.add('walk');
  if (sleeping) active.add('sleep');
  if (active.size === 0) active.add('idle');
  for (const state of CLIP_PRIORITY) {
    if (!active.has(state)) continue;
    if (available && !available.has(state)) continue;
    return state;
  }
  return 'idle';
}

/**
 * Should the `<img>` src be re-assigned to restart a one-shot animation?
 *
 * Re-assigning `src` is the only portable way to replay an animated image, and it re-downloads
 * from cache — visibly janky if done every frame. So: restart only when the state actually
 * changed, or when the same one-shot is asked for again after `minGap` has passed.
 */
export function shouldRestart(state, lastState, lastRestartAt, now, minGapMs = 250) {
  if (state !== lastState) return true;
  if (!ONE_SHOT.has(state)) return false;
  return now - (lastRestartAt ?? -Infinity) >= minGapMs;
}

/**
 * Advances a clip's playhead.
 *
 * The browser plays the animation; this tracks where we *expect* it to be, so a one-shot can be
 * timed out at a frame boundary instead of at a guessed wall-clock offset.
 *
 * Contract: `clip` is a **descriptor** (`{frames, frameMs, elapsed}`) and the return value is a
 * **reading** (`{index, elapsed, loops}`). Feed `elapsed` back into the descriptor — not the
 * whole result — or the frame metadata is lost:
 *
 *     const next = advanceClip({ ...clip, elapsed: clip.elapsed }, dt);
 *     clip.elapsed = next.elapsed;
 */
export function advanceClip(clip, dtMs) {
  if (!clip) return { index: 0, elapsed: 0, loops: 0 };
  // Every field is optional: a clip with no metadata yet must not produce NaN indices.
  const frameMs = Number.isFinite(clip.frameMs) && clip.frameMs > 0 ? clip.frameMs : 1000 / 17;
  const frames = Number.isFinite(clip.frames) && clip.frames > 0 ? clip.frames : 1;
  const total = frames * frameMs;
  const step = Number.isFinite(dtMs) && dtMs > 0 ? dtMs : 0;
  // `elapsed` is cumulative. Returning the modulo instead would make the caller's next call
  // recompute the offset from the wrong place and lose a whole loop.
  const elapsed = (Number.isFinite(clip.elapsed) ? clip.elapsed : 0) + step;
  const loops = Math.floor(elapsed / total);
  const index = Math.floor((elapsed % total) / frameMs);
  return { index, elapsed, loops };
}

/**
 * Places the pet in a viewport.
 *
 * `anchor` follows the sprite/pet convention: `bottom` means the image's bottom edge is at `y`.
 * The result is clamped so a scaled-up character cannot hang off the edge — the single most
 * common desktop-pet bug is a pet that walks half off screen and stays there.
 */
export function placeClipPet({
  viewW,
  viewH,
  naturalW,
  naturalH,
  scale = 1,
  anchor = 'bottom',
  x = 0.5,
  y = 1,
  facing = 1,
  margin = 4,
}) {
  const s = clamp(scale, 0.25, 4);
  const w = Math.max(1, naturalW * s);
  const h = Math.max(1, naturalH * s);
  const centerX = clamp(x, 0, 1) * viewW;
  const anchorY = clamp(y, 0, 1) * viewH;

  let left;
  if (w + margin * 2 <= viewW) {
    left = clamp(centerX - w / 2, margin, viewW - w - margin);
  } else {
    // Wider than the viewport: centre and accept the crop rather than pinning to a corner.
    left = (viewW - w) / 2;
  }

  let top;
  if (anchor === 'top') top = anchorY;
  else if (anchor === 'center') top = anchorY - h / 2;
  else top = anchorY - h;

  if (h + margin * 2 <= viewH) {
    // No bottom margin: a pet's feet should be able to touch the floor exactly, which is what
    // makes it look like it is standing on your screen rather than floating above it.
    top = clamp(top, margin, viewH - h);
  } else {
    top = viewH - h; // taller than the viewport: keep the feet in view
  }

  return { left, top, w, h, scale: s, facing: facing < 0 ? -1 : 1 };
}

/**
 * A wandering path that stays in `[minX, maxX]` and reverses at the walls.
 *
 * Two incommensurate sine waves give a path that does not visibly repeat, while staying bounded
 * by construction — no accumulating position, so it cannot drift off screen after hours.
 */
export function wanderPosition(t, seed = 1, minX = 0.08, maxX = 0.92) {
  const a = Math.sin(t * 0.31 + seed * 1.7);
  const b = Math.sin(t * 0.13 + seed * 4.1);
  const raw = (a * 0.7 + b * 0.3) * 0.5 + 0.5; // 0..1
  const x = lerp(minX, maxX, raw);
  // Facing follows the derivative of the same path, so the pet turns where it actually turns.
  const eps = 0.01;
  const ahead = (() => {
    const a2 = Math.sin((t + eps) * 0.31 + seed * 1.7);
    const b2 = Math.sin((t + eps) * 0.13 + seed * 4.1);
    return (a2 * 0.7 + b2 * 0.3) * 0.5 + 0.5;
  })();
  const facing = ahead >= raw ? 1 : -1;
  return { x: clamp(x, minX, maxX), facing };
}

/**
 * Deterministic blinking.
 *
 * A real blink is ~120 ms; `periodMs` is the *gap* between blinks. Each pet gets its own phase
 * from `seed`, so two pets on screen do not blink in lockstep, which reads as "animated GIF"
 * rather than "alive".
 */
export function pickBlink(t, seed = 1, periodMs = 4200, closeMs = 130) {
  const phase = ((t * 1000 + seed * 1237) % periodMs) / periodMs;
  const open = closeMs / periodMs;
  if (phase < open / 2) {
    // closing then opening: a triangle in 0..1
    const k = phase / (open / 2);
    return k < 0.5 ? k : 1 - k;
  }
  return 0;
}

/**
 * Decide the next state for a one-shot that has finished.
 *
 * One-shots must never leave the pet frozen on their last frame if the image fails to loop, so
 * the player always knows what to go back to.
 */
export function afterOneShot(fallback) {
  return fallback || 'idle';
}
