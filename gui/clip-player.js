/**
 * Clip player: realistic 2D characters that are not 3D.
 *
 * The reference implementation of this idea (Reminder Companion) renders its characters as
 * **animated WebP with an alpha channel** — full-frame AI-generated art, animated by an
 * image-to-video model. 264×280, 17 fps, 18 frames for a walk, 200 for a resting animation.
 *
 * That beats a procedural 3D rig on quality per byte by a wide margin, and it costs the runtime
 * almost nothing: the browser already decodes animated WebP, so "the player" is an `<img>`. No
 * three.js, no GPU, no rig. This module is the part that is *not* free:
 *
 *  - **State choreography.** Which clip plays when, how a walk hands back to idle, and how a
 *    reminder interrupts without snapping the frame index back to zero.
 *  - **Placement.** Anchor, scale, facing, and the wander path — with a viewport clamp so a pet
 *    cannot walk off the screen.
 *  - **Determinism.** Every timing decision takes `now` as an argument, so behaviour is testable
 *    without a clock and reproducible across machines.
 *
 * Pure functions live in `clip-player-math.js` and are tested under `node --test`; the DOM half
 * is here so the math stays dependency-free.
 */

import {
  clamp,
  damp,
  chooseClip,
  shouldRestart,
  advanceClip,
  placeClipPet,
  wanderPosition,
  pickBlink,
  CLIP_PRIORITY,
} from './clip-player-math.js';

/** States a clip pack may provide, in priority order (highest wins while active). */
export const CLIP_STATES = [
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

export class ClipPet {
  /**
   * @param {object} opts
   * @param {HTMLElement} opts.mount  positioned container (the whole desktop, usually)
   * @param {Record<string,string>} opts.clips  state -> URL for the animated image
   * @param {object} [opts.placement]  { scale, anchor, x, y, feet }
   * @param {number} [opts.seed]  makes wander/blink deterministic
   */
  constructor({ mount, clips, placement = {}, seed = 1 }) {
    this.mount = mount;
    this.clips = clips || {};
    this.placement = {
      scale: 1,
      anchor: 'bottom',
      x: 0.5, // fraction of viewport width
      y: 1, //   fraction of viewport height (feet)
      facing: 1,
      ...placement,
    };
    this.seed = seed;
    this.state = 'idle';
    this.heldUntil = 0;
    this.walking = false;
    this.now = 0;
    this.x = this.placement.x;
    this.lastRestart = Object.create(null);

    this.el = document.createElement('div');
    this.el.className = 'sp-clip-pet';
    this.el.setAttribute('role', 'img');
    this.el.setAttribute('aria-label', 'desktop companion');
    this.el.style.cssText =
      'position:absolute;left:0;top:0;pointer-events:auto;will-change:transform;' +
      'transform-origin:50% 100%;image-rendering:auto;';

    this.img = document.createElement('img');
    this.img.alt = '';
    this.img.draggable = false;
    // Decode off the main thread where supported: a 200-frame clip is ~11 MB of RGBA.
    this.img.decoding = 'async';
    this.img.style.cssText = 'display:block;width:100%;height:100%;object-fit:contain;';
    this.el.appendChild(this.img);
    mount.appendChild(this.el);

    this._applyPlacement();
    this.setState('idle', 0, true);
  }

  /** Swap the whole pack (species/character switch) without rebuilding the element. */
  setClips(clips, { reset = true } = {}) {
    this.clips = clips || {};
    if (reset) {
      this.setState('idle', this.now, true);
    }
  }

  /**
   * Play a one-shot state for `ms`, then fall back.
   *
   * A reminder must be able to interrupt a walk *without* the walk's frame index resetting —
   * otherwise the pet visibly teleports mid-cycle every time it is reminded. Only the `<img src>`
   * changes; the browser keeps each animation's own playhead.
   */
  play(state, ms = 3000, now = this.now) {
    if (!this.clips[state]) {
      // Degrade visibly: fall back to remind, then idle, rather than showing a broken image.
      state = this.clips.remind ? 'remind' : 'idle';
    }
    this.heldUntil = now + ms;
    this.setState(state, now);
  }

  setWalking(on, now = this.now) {
    this.walking = Boolean(on) && Boolean(this.clips.walk);
    if (this.walking && now >= this.heldUntil) {
      this.setState('walk', now);
    } else if (!this.walking && now >= this.heldUntil) {
      this.setState('idle', now);
    }
  }

  get isWalkingScreenWise() {
    return this.walking && this.state === 'walk';
  }

  /**
   * Advance one frame of logic. `dt` is seconds; `now` is a monotonic clock reading.
   */
  update(dt, now = this.now) {
    this.now = now;
    if (now < this.heldUntil) {
      // A held state (remind/celebrate/…) is playing; nothing else may take over.
      return;
    }
    const next = chooseClip({ walking: this.walking, sleeping: this.clips.sleep ? this.state === 'sleep' : false });
    if (next !== this.state) {
      this.setState(next, now);
    }

    // Walk across the screen, then settle.
    if (this.isWalkingScreenWise) {
      const w = wanderPosition(now / 1000, this.seed, 0.08, 0.92);
      this.x = damp(this.x, w.x, 0.9, dt);
      this.placement.facing = w.facing;
    } else if (this.restX != null) {
      this.x = damp(this.x, this.restX, 1.4, dt);
    }
    this._applyPlacement();
  }

  /** Where the pet sits when it is not walking. */
  setRestX(x) {
    this.restX = clamp(x, 0.05, 0.95);
  }

  _applyPlacement() {
    const box = this.mount.getBoundingClientRect();
    const p = placeClipPet({
      viewW: box.width || 800,
      viewH: box.height || 600,
      naturalW: this.naturalW || 264,
      naturalH: this.naturalH || 280,
      scale: this.placement.scale,
      anchor: this.placement.anchor,
      x: this.x,
      y: this.placement.y,
      facing: this.placement.facing,
      margin: 4,
    });
    this.el.style.width = `${p.w}px`;
    this.el.style.height = `${p.h}px`;
    this.el.style.transform =
      `translate3d(${p.left}px, ${p.top}px, 0) scaleX(${p.facing})`;
  }

  _setState(state, now) {
    if (!this.clips[state]) return;
    if (state === this.state && this.img.src) return;
    this.state = state;
    this.img.src = this.clips[state];
    this.lastRestart[state] = now;
    if (!this.naturalW) {
      // Read once, then keep: the pack is authored at one size.
      const probe = new Image();
      probe.onload = () => {
        this.naturalW = probe.naturalWidth || 264;
        this.naturalH = probe.naturalHeight || 280;
        this._applyPlacement();
      };
      probe.src = this.clips[state];
    }
  }

  /** Kept as a method name for symmetry with the 3D `Companion` class. */
  setState(state, now = this.now, force = false) {
    if (force) this.state = null;
    this._setState(state, now);
  }

  dispose() {
    this.el.remove();
    this.img.src = '';
  }
}

export { CLIP_PRIORITY };
