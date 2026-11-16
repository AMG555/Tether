/**
 * companion-overlay.js
 * Runs inside the transparent companion-overlay window (full-screen).
 *
 * Rendering priority:
 *   1. WebP sprite frames from installed companion pack (via IPC)
 *   2. Procedural canvas fallback (custom companions / no pack installed)
 *
 * Walk behaviour: companion auto-walks across the bottom of the screen
 * left → right then right → left. User can drag it to any X/Y position.
 *
 * Click-through: the full canvas has pointer-events:none so the OS treats the
 * transparent area as click-through.  A small #hit-zone div tracks the character
 * position in CSS (updated after every render frame) and captures mouse events.
 * Drag updates walkX/walkY directly in JS — no native window drag, no IPC needed.
 *
 * Reactive status: getCaptureStatus is polled every STATUS_POLL_MS ms.  When
 * capturing is active the companion shows a live "brain working" reaction and
 * the speech bubble surfaces real event counts.
 *
 * All Tauri IPC calls must go through api.js (architecture rule).
 */

import { getCompanionClip, getActiveCompanion, getCaptureStatus, setCompanionPassthrough } from './api.js';
import { createBrain } from './companion-brain.js';

// ─── Constants ────────────────────────────────────────────────────────────────

const WALK_SPEED_PX     = 2.2;    // pixels per frame (faster, more lively)
const WALK_PAUSE_MS     = 4000;   // pause at edge before turning
const IDLE_PAUSE_CHANCE = 0.002;  // per-frame chance to pause mid-walk
const CHAR_SCALE        = 1.55;   // global scale for procedural character
const CHAR_MARGIN       = 60;     // px from each horizontal edge — walk boundary
const CHAR_WIDTH        = 80;     // approximate half-width for hit-testing
const CHAR_HEIGHT       = 220;    // approximate full height for hit-testing
const COMPANION_TARGET_H = 220;   // render height in logical CSS pixels
const STATUS_POLL_MS    = 4000;   // how often to poll getCaptureStatus (ms)
const DRAG_RESUME_MS    = 2000;   // ms after drop before auto-walk resumes

// ─── Free Roam mode ───────────────────────────────────────────────────────────
// When enabled the companion walks to random destinations anywhere on screen
// instead of only patrolling the bottom edge.  Stored in localStorage so it
// survives a companion reload.  Completely optional — default is off.

const ROAM_STORAGE_KEY   = 'tether_companion_free_roam';
const ROAM_SPEED_PX      = 1.8;   // slightly slower for diagonal movement (feels calmer)
const ROAM_ARRIVE_DIST   = 40;    // px — "close enough" radius to count as arrived
const ROAM_PAUSE_MIN_MS  = 2000;  // minimum pause at each waypoint
const ROAM_PAUSE_MAX_MS  = 6000;  // maximum pause at each waypoint
const ROAM_MARGIN_X      = 80;    // px from horizontal edges (keep companion visible)
const ROAM_MARGIN_TOP    = CHAR_HEIGHT + 20; // px from top (clear the head)
const ROAM_MARGIN_BOT    = 20;    // px from bottom

let freeRoam = typeof localStorage !== 'undefined'
  && localStorage.getItem(ROAM_STORAGE_KEY) === 'true';

// Current roam waypoint the companion is walking toward
let _roamTargetX = 0;
let _roamTargetY = 0;
let _roamPauseUntil = 0; // timestamp — 0 means pick a new waypoint now

// ─── Companion data ───────────────────────────────────────────────────────────

const DEFAULT_COMPANION = {
  id: 'grace',
  name: 'Grace',
  avatarType: 'procedural',
  colors: { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' },
};

// companion starts as the default and is replaced asynchronously once IPC loads.
let companion = { ...DEFAULT_COMPANION };

// Load the active companion from the shared file via IPC (through api.js).
// This is the only reliable cross-WebView source of truth — localStorage is
// isolated per WebView and cannot be shared between the main window and overlay.
async function loadActiveCompanionFromIPC() {
  try {
    const state = await getActiveCompanion();
    if (state && state.id) return state;
  } catch { /* non-fatal — fall back to default */ }
  return DEFAULT_COMPANION;
}

// ─── Canvas + hit-zone setup ──────────────────────────────────────────────────

const canvas  = document.getElementById('companion-canvas');
const ctx     = canvas.getContext('2d');
const hitZone = document.getElementById('hit-zone');

// Logical dimensions — always CSS pixels.  All draw code uses these.
let W = window.innerWidth;
let H = window.innerHeight;

let _appliedDpr = 1;
function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  const w = Math.round(window.innerWidth * dpr);
  const h = Math.round(window.innerHeight * dpr);
  W = window.innerWidth;
  H = window.innerHeight;
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width  = w;
    canvas.height = h;
    ctx.scale(dpr, dpr);
    _appliedDpr = dpr;
  }
}
resizeCanvas();

window.addEventListener('resize', () => {
  resizeCanvas();
  if (freeRoam) {
    walkX = Math.max(ROAM_MARGIN_X, Math.min(W - ROAM_MARGIN_X, walkX));
    walkY = Math.max(ROAM_MARGIN_TOP, Math.min(H - ROAM_MARGIN_BOT, walkY));
    // Current roam target may now be out-of-bounds — pick a fresh one
    pickRoamTarget();
  } else {
    walkX = Math.max(CHAR_MARGIN, Math.min(W - CHAR_MARGIN, walkX));
    walkY = Math.max(CHAR_HEIGHT + 10, Math.min(H - 10, walkY));
  }
  updateHitZone();
});

// Move the hit-zone div so it covers the character's current bounding box.
function updateHitZone() {
  const hw = CHAR_WIDTH * 2;
  const hh = CHAR_HEIGHT;
  hitZone.style.left   = `${Math.round(walkX - hw / 2)}px`;
  hitZone.style.top    = `${Math.round(walkY - hh)}px`;
  hitZone.style.bottom = '';
  hitZone.style.width  = `${hw}px`;
  hitZone.style.height = `${hh}px`;
}

// ─── State machine ────────────────────────────────────────────────────────────

function getWalkStates() {
  // Use the actions list from the IPC-loaded companion data so the state machine
  // picks only actions that have been enabled for this companion.
  // companion.actions comes from the manifest.json / fallback list via commands.rs.
  if (companion.actions && companion.actions.length > 0) {
    // Weight idle and walk more heavily so they appear naturally in the mix.
    const weighted = [];
    for (const a of companion.actions) {
      if (a === 'idle' || a === 'walk') { weighted.push(a, a); } else { weighted.push(a); }
    }
    return weighted;
  }
  return ['idle', 'idle', 'walk', 'walk', 'wave', 'happy', 'water', 'stretch'];
}

function getActionQuotes() {
  // Quote overrides live in localStorage only in the main window where the user
  // types them.  The overlay can't share that store, so we just use the defaults.
  return {};
}

let WALK_STATES = getWalkStates();
let currentState = 'idle';
let nextStateTimer = 0;
let walkDir = 1;           // 1 = right, -1 = left
let walkX = W / 2;         // start at horizontal centre so companion is visible immediately
let walkY = H;             // feet Y position (bottom of screen by default)
let walking = false;
let pauseTimer = 0;
let animTime = 0;
let rafId = null;

// Spring physics for smooth vertical bounce
let springY = 0;
let springVY = 0;
const SPRING_K = 18;
const SPRING_DAMP = 0.72;

// Speech bubble state
let bubbleText = '';
let bubbleAlpha = 0;
let bubbleTimer = 0;

// Reactive capture-status state
let _captureStatus = { capturing: false, eventsSeen: 0, draftsSaved: 0 };
let _lastEventsSeen = 0;    // detect new events between polls
let _statusIndicator = '';  // emoji/text shown on the character when capturing
let _dragResumeAt = 0;      // timestamp after which auto-walk re-enables post-drag

function showBubble(text, durationMs = 3000) {
  bubbleText = text;
  bubbleAlpha = 1;
  bubbleTimer = performance.now() + durationMs;
}

// ─── Session Brain ────────────────────────────────────────────────────────────
// Pure-logic model imported from companion-brain.js.  The onCelebrate callback
// drives the companion animation (state change + spring + bubble) so the brain
// module itself has no DOM dependency and can be unit-tested in Node.

const brain = createBrain({
  storage: typeof localStorage !== 'undefined' ? localStorage : null,
  onCelebrate(_milestone, text) {
    // Delay 400 ms so a celebration doesn't clash with an in-progress poll bubble.
    setTimeout(() => {
      currentState = 'happy';
      walking = false;
      springVY = -14;
      nextStateTimer = performance.now() + 4000;
      showBubble(text, 3800);
      loadSprite(companion.id, 'happy');
    }, 400);
  },
});

// Returning-user streak greeting — fires once per day, 5 s after first capture.
// The brain emits the streak count via streakDays; we show the bubble here so
// that the greeting text can reference showBubble (a DOM-layer concern).
let _streakGreetingShown = false;
function _maybeShowStreakGreeting() {
  if (_streakGreetingShown) return;
  if (brain.streakDays > 1) {
    _streakGreetingShown = true;
    setTimeout(() => showBubble(`🌟 Day ${brain.streakDays} — welcome back!`, 3500), 5000);
  }
}

// ─── Free Roam helpers ────────────────────────────────────────────────────────

/** Pick a random on-screen waypoint the companion will walk toward. */
function pickRoamTarget() {
  _roamTargetX = ROAM_MARGIN_X + Math.random() * (W - ROAM_MARGIN_X * 2);
  _roamTargetY = ROAM_MARGIN_TOP + Math.random() * (H - ROAM_MARGIN_TOP - ROAM_MARGIN_BOT);
  _roamPauseUntil = 0; // start walking immediately
}

/** Step companion one frame toward the current roam target. Returns true while still walking. */
function stepRoam(timestamp) {
  // Waiting at a waypoint
  if (_roamPauseUntil > 0) {
    if (timestamp < _roamPauseUntil) return false; // still pausing
    _roamPauseUntil = 0;
    pickRoamTarget();
    return false;
  }

  const dx = _roamTargetX - walkX;
  const dy = _roamTargetY - walkY;
  const dist = Math.sqrt(dx * dx + dy * dy);

  if (dist < ROAM_ARRIVE_DIST) {
    // Arrived — pause then pick a new destination
    const pauseMs = ROAM_PAUSE_MIN_MS + Math.random() * (ROAM_PAUSE_MAX_MS - ROAM_PAUSE_MIN_MS);
    _roamPauseUntil = timestamp + pauseMs;
    return false;
  }

  // Move toward target — normalise direction and scale by speed
  walkX += (dx / dist) * ROAM_SPEED_PX;
  walkY += (dy / dist) * ROAM_SPEED_PX;
  // Keep inside safe bounds
  walkX = Math.max(ROAM_MARGIN_X, Math.min(W - ROAM_MARGIN_X, walkX));
  walkY = Math.max(ROAM_MARGIN_TOP, Math.min(H - ROAM_MARGIN_BOT, walkY));
  // Update face direction based on horizontal movement
  walkDir = dx >= 0 ? 1 : -1;
  return true; // still walking
}

/** Sync the roam button visual state to the current freeRoam value. */
function syncRoamBtn() {
  const btn = document.getElementById('roam-btn');
  if (!btn) return;
  btn.classList.toggle('roam-on', freeRoam);
  btn.textContent = freeRoam ? '🗺️' : '🌐';
  btn.title = freeRoam
    ? 'Free Roam ON — click to return to bottom-walk'
    : 'Free Roam OFF — click to let companion roam the screen';
}

// ─── Reactive status polling ──────────────────────────────────────────────────

async function pollCaptureStatus() {
  try {
    const s = await getCaptureStatus();
    if (!s) return;

    const wasCapturing = _captureStatus.capturing;
    _captureStatus = s;

    // Feed the brain before we act on the data so its velocity and milestone
    // calculations are current for any reactive decisions below.
    if (s.capturing) brain.observe(s.eventsSeen, s.draftsSaved, s.capturing);

    // New events arrived since last poll → companion reacts
    if (s.eventsSeen > _lastEventsSeen && s.eventsSeen - _lastEventsSeen >= 10) {
      const delta = s.eventsSeen - _lastEventsSeen;
      // Brain-aware reactive messages: use velocity context rather than a fixed pool
      const velocity = brain.currentVelocity();
      let msg;
      if (velocity > 60) {
        msg = `🔥 ${delta} keystrokes — you're in the zone!`;
      } else if (s.draftsSaved > 0 && Math.random() < 0.35) {
        msg = `💾 ${s.draftsSaved} draft${s.draftsSaved > 1 ? 's' : ''} saved`;
      } else {
        const pool = [
          `📝 ${delta} new events!`,
          `⚡ ${delta} keystrokes captured`,
          `🧠 ${s.eventsSeen} events total`,
        ];
        msg = pool[Math.floor(Math.random() * pool.length)];
      }
      showBubble(msg, 3000);
      springVY = -7;
    }
    _lastEventsSeen = s.eventsSeen;

    // Companion just started capturing → celebrate + check returning-user greeting
    if (!wasCapturing && s.capturing) {
      currentState = 'happy';
      walking = false;
      nextStateTimer = performance.now() + 3000;
      springVY = -12;
      showBubble('🔴 Recording started!', 2500);
      loadSprite(companion.id, 'happy');
      _maybeShowStreakGreeting();
    }

    // Companion just stopped capturing → note it
    if (wasCapturing && !s.capturing) {
      showBubble('⏹️ Capture paused', 2000);
    }

    // Set the indicator dot shown on the character body
    _statusIndicator = s.capturing ? '🔴' : '';
  } catch { /* non-fatal */ }
}

// Start polling immediately and every STATUS_POLL_MS thereafter.
pollCaptureStatus();
setInterval(pollCaptureStatus, STATUS_POLL_MS);

// ─── Animated WebP sprite engine ─────────────────────────────────────────────
//
// ctx.drawImage(img) always stamps the current frame of a live DOM <img> element.
// The browser advances animated-WebP frames internally on its own clock, so each
// RAF we simply draw the same <img> and get the correct frame for free.
// All sprite <img> elements are appended off-screen (position:fixed, left:-9999px)
// so the browser keeps their animation clock running.

/** @type {Record<string, HTMLImageElement>} key = `${companionId}:${state}` */
const spriteCache = {};
/** @type {Record<string, boolean>} */
const spriteLoading = {};

/** Hidden container that keeps sprite <img> elements alive in the DOM. */
// The sprite container holds the live <img> elements whose src is the animated WebP.
// It MUST be visible (non-zero size, non-display:none) so Chromium/WebView2 keeps
// advancing animation frames.  opacity:0 + pointer-events:none hides it from the
// user while keeping it in the rendering pipeline — width:0 or display:none would
// freeze the animation on frame 0, making every sprite appear as a still image.
const spriteContainer = (() => {
  const el = document.createElement('div');
  el.setAttribute('aria-hidden', 'true');
  el.style.cssText = 'position:fixed;left:0;top:0;opacity:0;pointer-events:none;overflow:hidden;';
  document.body.appendChild(el);
  return el;
})();

/** Sentinel placed in spriteCache for a permanently missing/broken sprite. */
const SPRITE_MISSING = null;

async function loadSprite(companionId, state) {
  const key = `${companionId}:${state}`;
  if (key in spriteCache) return spriteCache[key]; // includes SPRITE_MISSING sentinel
  if (spriteLoading[key]) return undefined;
  spriteLoading[key] = true;
  try {
    // Check localStorage for a custom WebP uploaded via the Behaviors tab.
    // Keys checked in preference order:
    //   1. companion-specific: tether-custom-webp-{companionId}-{state}
    //   2. action-generic:     tether-custom-webp-{state}
    let dataUrl = null;
    try {
      dataUrl = localStorage.getItem(`tether-custom-webp-${companionId}-${state}`)
             || localStorage.getItem(`tether-custom-webp-${state}`)
             || null;
    } catch { /* storage unavailable */ }
    if (!dataUrl) dataUrl = await getCompanionClip(companionId, state);
    if (dataUrl) {
      const img = new Image();
      // Insert into DOM *before* setting src so the browser animation clock
      // starts immediately when the image finishes loading.
      spriteContainer.appendChild(img);
      img.src = dataUrl;
      await new Promise((res) => { img.onload = res; img.onerror = res; });
      if (img.naturalWidth > 0) {
        spriteCache[key] = img;
        delete spriteLoading[key];
        return img;
      }
      // Image loaded but reported zero size — broken file, remove orphan
      spriteContainer.removeChild(img);
    }
  } catch { /* fall through */ }
  // Mark as permanently missing so we never flood IPC with retries
  spriteCache[key] = SPRITE_MISSING;
  delete spriteLoading[key];
  return SPRITE_MISSING;
}

// Pre-warm the most common actions immediately so the character appears without a
// procedural flash on first state transition.
function prewarmSprites(id) {
  const common = ['idle', 'walk', 'wave', 'happy', 'water', 'stretch', 'sad', 'eyes'];
  for (const action of common) loadSprite(id, action);
}
prewarmSprites(companion.id);

// ─── State helpers ────────────────────────────────────────────────────────────

const DEFAULT_QUOTES = {
  idle:    '',
  walk:    '',
  wave:    '👋 Hey there!',
  water:   '💧 Stay hydrated!',
  happy:   '✨ Looking good!',
  stretch: '🧘 Stretch time!',
  sad:     '😴 Rest your eyes…',
  eyes:    '👁️ Eye break — look away!',
};

function pickNextState() {
  WALK_STATES = getWalkStates();
  if (WALK_STATES.length === 0) WALK_STATES = ['idle'];

  // ── Brain-weighted state selection ──────────────────────────────────────
  // Build a weighted pool: each state appears proportional to its brain weight.
  // States absent from the weights object keep their default count of 1.
  const weights = brain.stateWeights();
  const pool = [];
  for (const s of WALK_STATES) {
    const w = weights[s] ?? 1;
    // Convert to integer slots (round up to at least 1 so no state disappears)
    const slots = Math.max(1, Math.round(w));
    for (let i = 0; i < slots; i++) pool.push(s);
  }
  currentState = pool[Math.floor(Math.random() * pool.length)];
  // ────────────────────────────────────────────────────────────────────────

  walking = currentState === 'walk';
  // Schedule next transition 5–12 s from now.
  // Use performance.now() so it stays consistent with the RAF timestamp clock.
  nextStateTimer = performance.now() + (5000 + Math.random() * 7000);
  // Clear any pending edge-pause timer so it doesn't fight the new state
  pauseTimer = 0;
  loadSprite(companion.id, currentState);

  // Brain-aware message: prefer a contextual line; fall back to static default.
  const quotes = getActionQuotes();
  const text = quotes[currentState]
    || brain.messageFor(currentState)
    || DEFAULT_QUOTES[currentState]
    || '';
  if (text) showBubble(text, 2800);
}

// Initialise nextStateTimer to a value safely in the future so the first
// renderFrame call doesn't immediately fire pickNextState a second time.
// (lastTimestamp starts at 0 so the first dt is large; RAF timestamp on first
// call is already in the thousands of ms, which would exceed nextStateTimer=0.)
pickNextState();

// ─── Render loop ──────────────────────────────────────────────────────────────

let lastTimestamp = 0;

function renderFrame(timestamp) {
  rafId = null;
  const dt = Math.min(timestamp - lastTimestamp, 50);
  lastTimestamp = timestamp;
  animTime += dt * 0.001;

  // State transition — but not while dragging and not while an edge-pause is
  // pending (let the walk resume first, then allow a fresh state pick).
  // In free-roam mode pauses are handled by the roam state machine, so skip
  // the standard pauseTimer guard.
  if (timestamp > nextStateTimer && !_dragActive && (freeRoam || pauseTimer === 0)) pickNextState();

  // Walk position — blocked during drag and briefly after drop
  const walkAllowed = !_dragActive && (_dragResumeAt === 0 || timestamp > _dragResumeAt);

  if (freeRoam && walkAllowed) {
    // ── Free Roam mode: walk toward random waypoints all over the screen ──────
    const isMoving = stepRoam(timestamp);
    if (isMoving) {
      // Show walk sprite/state while moving
      if (currentState !== 'walk') {
        currentState = 'walk';
        walking = true;
        loadSprite(companion.id, 'walk');
      }
      // Occasional spring bounce to keep it lively
      if (Math.random() < 0.006) springVY = -5;
    } else {
      // Pausing at waypoint — play idle or a random non-walk action
      if (walking) {
        walking = false;
        // Let the normal state machine choose what to show while stopped
        pickNextState();
      }
    }
  } else {
    // ── Standard mode: patrol along the bottom edge ──────────────────────────
    if (walking && walkAllowed) {
      if (Math.random() < IDLE_PAUSE_CHANCE) {
        walking = false;
        currentState = 'idle';
        nextStateTimer = performance.now() + 3000;
      } else {
        walkX += walkDir * WALK_SPEED_PX;
        if (walkX > W - CHAR_MARGIN) {
          walkX = W - CHAR_MARGIN;
          walkDir = -1;
          walking = false;
          pauseTimer = timestamp + WALK_PAUSE_MS;
          currentState = 'idle';
          nextStateTimer = performance.now() + WALK_PAUSE_MS + 1000;
        } else if (walkX < CHAR_MARGIN) {
          walkX = CHAR_MARGIN;
          walkDir = 1;
          walking = false;
          pauseTimer = timestamp + WALK_PAUSE_MS;
          currentState = 'idle';
          nextStateTimer = performance.now() + WALK_PAUSE_MS + 1000;
        }
      }
    } else if (walkAllowed && pauseTimer > 0 && timestamp > pauseTimer) {
      pauseTimer = 0;
      walking = true;
      currentState = 'walk';
      nextStateTimer = performance.now() + 9000;
    }
  }

  // Spring physics — idle gets a small occasional nudge so it never looks frozen
  if (currentState === 'idle' && Math.abs(springVY) < 0.1 && Math.abs(springY) < 0.1) {
    if (Math.random() < 0.003) springVY = -4;
  }
  if (currentState === 'walk' && Math.abs(springVY) < 0.1 && Math.abs(springY) < 0.1) {
    if (Math.random() < 0.008) springVY = -6;
  }
  if (currentState === 'happy' && Math.random() < 0.012) springVY = -10;

  const springForce = -SPRING_K * springY;
  springVY = (springVY + springForce * dt * 0.001) * SPRING_DAMP;
  springY += springVY;
  if (Math.abs(springY) < 0.05 && Math.abs(springVY) < 0.05) {
    springY = 0; springVY = 0;
  }

  // Speech bubble fade-out
  if (bubbleAlpha > 0 && timestamp > bubbleTimer) {
    bubbleAlpha = Math.max(0, bubbleAlpha - dt * 0.002);
  }

  ctx.clearRect(0, 0, W, H);
  drawCompanion();
  if (bubbleAlpha > 0.01) drawBubble();
  updateHitZone();

  rafId = requestAnimationFrame(renderFrame);
}

function drawBubble() {
  const bx = walkX;
  const by = walkY - COMPANION_TARGET_H + 40 + springY;
  const pad = 12;
  ctx.font = 'bold 13px -apple-system, Segoe UI, sans-serif';
  const tw = ctx.measureText(bubbleText).width;
  const bw = tw + pad * 2;
  const bh = 32;

  ctx.save();
  ctx.globalAlpha = Math.min(bubbleAlpha, 0.95);

  // Clamp bubble horizontally so it stays on-screen
  const clampedBx = Math.max(bw / 2 + 8, Math.min(W - bw / 2 - 8, bx));
  const rx = clampedBx - bw / 2;
  const ry = by - bh;

  // Drop shadow
  ctx.shadowColor = 'rgba(0,0,0,0.12)';
  ctx.shadowBlur = 8;
  ctx.shadowOffsetY = 2;

  ctx.fillStyle = '#fff';
  ctx.strokeStyle = 'rgba(0,0,0,0.10)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(rx, ry, bw, bh, 10);
  ctx.fill();
  ctx.stroke();

  ctx.shadowColor = 'transparent';
  ctx.shadowBlur = 0;
  ctx.shadowOffsetY = 0;

  // Tail — point from clamped bubble toward actual companion centre
  const tailX = Math.max(rx + 12, Math.min(rx + bw - 12, bx));
  ctx.fillStyle = '#fff';
  ctx.beginPath();
  ctx.moveTo(tailX - 7, by);
  ctx.lineTo(tailX, by + 10);
  ctx.lineTo(tailX + 7, by);
  ctx.closePath();
  ctx.fill();

  ctx.fillStyle = '#1f2328';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(bubbleText, clampedBx, ry + bh / 2);

  ctx.restore();
}

// Draw the live status indicator (e.g. 🔴 when capturing) above the companion
function drawStatusIndicator() {
  if (!_statusIndicator) return;
  ctx.save();
  ctx.font = '14px -apple-system, Segoe UI, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.globalAlpha = 0.9;
  ctx.fillText(_statusIndicator, walkX + CHAR_WIDTH * 0.6, walkY - COMPANION_TARGET_H + springY - 8);
  ctx.restore();
}

/** Draw a loaded WebP sprite image at the companion's current position.
 *
 * flip — mirror horizontally (for right-to-left walk on a right-facing sprite).
 *         Applied as a canvas transform so the animated WebP frames are still
 *         rendered correctly — just mirrored at draw time each RAF tick.
 */
function drawSpriteImg(img, flip) {
  const scale = Math.min(COMPANION_TARGET_H / img.naturalHeight, (W * 0.5) / img.naturalWidth);
  const dw = img.naturalWidth * scale;
  const dh = img.naturalHeight * scale;
  const dx = walkX - dw / 2;
  const dy = walkY - dh + springY;
  ctx.save();
  if (flip) {
    // Translate so the mirror axis is the character's centre X, then scale -1.
    // This keeps the character centred at walkX after the flip.
    ctx.translate(walkX * 2, 0);
    ctx.scale(-1, 1);
  }
  // drawImage on an animated WebP always stamps the browser's current frame —
  // the browser advances frames on its own clock for free — no manual frame
  // stepping needed.  Every RAF tick the latest decoded frame is stamped here.
  ctx.drawImage(img, dx, dy, dw, dh);
  ctx.restore();
}

function drawCompanion() {
  const spriteKey = `${companion.id}:${currentState}`;

  // ── Flip logic ──────────────────────────────────────────────────────────────
  // Walk WebP sprites are assumed to face RIGHT (the most common convention).
  // When the companion is walking LEFT we mirror horizontally at draw time so a
  // single walk.webp serves both directions — no separate asset needed.
  //
  // For all other states (idle, wave, water, etc.) we keep the asset's natural
  // facing direction.  Flipping non-walk states based on walkDir caused the
  // companion to randomly mirror during idle which looked wrong.
  const shouldFlip = currentState === 'walk' && walkDir === -1;

  const img = spriteCache[spriteKey]; // undefined = not fetched yet, null = missing

  if (img) {
    // Best case: exact sprite for this state is loaded and ready.
    drawSpriteImg(img, shouldFlip);
    drawStatusIndicator();
    return;
  }

  if (img === undefined) {
    // Not yet in cache — kick off a load (loadSprite deduplicates concurrent requests).
    loadSprite(companion.id, currentState);
  }
  // img === null → permanently missing, fall through to bridge/fallback.

  // Bridge: use the idle sprite for any state that hasn't loaded yet.
  // The idle WebP is pre-warmed first (see prewarmSprites) so it is almost
  // always available within the first animation frame.  Showing it here means
  // the companion is NEVER a blank canvas — it always feels alive.
  // Don't flip the bridge idle sprite for non-walk states.
  const idleKey = `${companion.id}:idle`;
  const idleImg = spriteCache[idleKey];
  if (idleImg) {
    // For the bridge we never flip — idle is always shown facing right.
    drawSpriteImg(idleImg, false);
    drawStatusIndicator();
    return;
  }

  // Last resort: procedural animated character (covers brand-new companions
  // before any sprites have loaded, or companions with no WebP assets at all).
  drawProceduralCompanion(shouldFlip);
  drawStatusIndicator();
}

function drawProceduralCompanion(flip) {
  const colors = companion.colors || DEFAULT_COMPANION.colors;
  const cx = walkX;
  // Feet at walkY, spring offset shifts entire body
  const baseY = walkY;
  const sc = CHAR_SCALE;

  let bob = Math.sin(animTime * 2.5) * 2.5 * sc + springY;
  let waveAngle = 0;
  let walkPhase = 0;
  let eyeBlink = Math.sin(animTime * 0.7) > 0.96 ? 0.08 : 1.0;
  let breathe = Math.sin(animTime * 1.2) * 1.5 * sc;

  if (currentState === 'walk') {
    walkPhase = Math.sin(animTime * 7);
    bob = Math.abs(Math.sin(animTime * 7)) * 6 * sc + springY;
    breathe = 0;
  } else if (currentState === 'wave') {
    waveAngle = Math.sin(animTime * 9) * 0.6;
    bob += Math.sin(animTime * 3) * 2 * sc;
  } else if (currentState === 'happy') {
    bob = Math.abs(Math.sin(animTime * 9)) * 11 * sc + springY;
    waveAngle = Math.sin(animTime * 13) * 0.45;
    breathe = 0;
  } else if (currentState === 'stretch') {
    bob = Math.sin(animTime * 1.5) * 4 * sc + springY;
    waveAngle = -Math.abs(Math.sin(animTime * 1.5)) * 0.9;
  } else if (currentState === 'sad') {
    bob = -Math.abs(Math.sin(animTime * 1.2)) * 3 * sc + springY;
    eyeBlink = 0.3;
  } else if (currentState === 'water') {
    // Bend forward slightly, one arm raised as if holding a bottle
    bob = Math.sin(animTime * 2) * 2 * sc + springY;
    waveAngle = Math.sin(animTime * 1.5) * 0.2 + 0.5; // right arm raised
  } else if (currentState === 'eyes') {
    // Wide eyes, slow blink — "look away from screen" reminder
    eyeBlink = 0.5 + Math.abs(Math.sin(animTime * 0.4)) * 0.5;
    bob = Math.sin(animTime * 1.0) * 1 * sc + springY;
  }

  ctx.save();
  if (flip) { ctx.translate(cx * 2, 0); ctx.scale(-1, 1); }

  ctx.fillStyle = 'rgba(0,0,0,0.14)';
  ctx.beginPath();
  ctx.ellipse(cx, baseY + 4, 30 * sc, 7 * sc, 0, 0, Math.PI * 2);
  ctx.fill();

  const legSpread = 11 * sc;
  const legW = 10 * sc;
  const legH = 18 * sc;
  ctx.fillStyle = colors.accent || '#c2331a';
  const legL = walkPhase * 9 * sc;
  const legR = -walkPhase * 9 * sc;
  ctx.beginPath();
  ctx.roundRect(cx - legSpread - legW * 0.5, baseY - legH - bob + legL, legW, legH + 4 * sc, 3 * sc);
  ctx.fill();
  ctx.beginPath();
  ctx.roundRect(cx + legSpread - legW * 0.5, baseY - legH - bob + legR, legW, legH + 4 * sc, 3 * sc);
  ctx.fill();

  const bodyW = 46 * sc;
  const bodyH = 50 * sc + breathe;
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(cx - bodyW / 2, baseY - legH - bodyH - bob, bodyW, bodyH, 8 * sc);
  ctx.fill();

  const badgeW = 20 * sc;
  const badgeH = 26 * sc;
  ctx.fillStyle = colors.accent || '#c2331a';
  ctx.beginPath();
  ctx.roundRect(cx - badgeW / 2, baseY - legH - bodyH * 0.65 - bob, badgeW, badgeH, 4 * sc);
  ctx.fill();
  ctx.fillStyle = 'rgba(255,255,255,0.18)';
  ctx.beginPath();
  ctx.roundRect(cx - badgeW / 2 + 2 * sc, baseY - legH - bodyH * 0.65 - bob + 2 * sc, badgeW * 0.4, badgeH * 0.45, 2 * sc);
  ctx.fill();

  ctx.save();
  ctx.translate(cx - bodyW / 2, baseY - legH - bodyH * 0.75 - bob);
  const leftArmRot =
    currentState === 'stretch' ? -1.4 + Math.sin(animTime * 1.5) * 0.15
    : currentState === 'wave'  ? -1.15 + waveAngle
    : currentState === 'eyes'  ? 0.0                  // arms relaxed for eye break
    : 0.18 - walkPhase * 0.28;
  ctx.rotate(leftArmRot);
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(-9 * sc, 0, 9 * sc, 26 * sc, 4 * sc);
  ctx.fill();
  ctx.restore();

  ctx.save();
  ctx.translate(cx + bodyW / 2, baseY - legH - bodyH * 0.75 - bob);
  const rightArmRot =
    currentState === 'stretch' ? 1.4 - Math.sin(animTime * 1.5) * 0.15
    : currentState === 'wave'  ? 1.1 - waveAngle
    : currentState === 'water' ? -(0.8 + waveAngle)   // right arm raised with bottle
    : currentState === 'eyes'  ? 0.0                  // arms relaxed for eye break
    : -0.18 + walkPhase * 0.28;
  ctx.rotate(rightArmRot);
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(0, 0, 9 * sc, 26 * sc, 4 * sc);
  ctx.fill();
  ctx.restore();

  const headW = 40 * sc;
  const headH = 32 * sc;
  const headY = baseY - legH - bodyH - headH - bob + 4 * sc;
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(cx - headW / 2, headY, headW, headH, 8 * sc);
  ctx.fill();

  ctx.fillStyle = colors.accent || '#c2331a';
  ctx.beginPath();
  ctx.moveTo(cx - 16 * sc, headY + 2 * sc);
  ctx.lineTo(cx - 10 * sc, headY - 14 * sc);
  ctx.lineTo(cx - 4 * sc, headY + 2 * sc);
  ctx.closePath();
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(cx + 4 * sc,  headY + 2 * sc);
  ctx.lineTo(cx + 10 * sc, headY - 14 * sc);
  ctx.lineTo(cx + 16 * sc, headY + 2 * sc);
  ctx.closePath();
  ctx.fill();

  const eyeY = headY + headH * 0.42;
  const eyeW = 8 * sc;
  const eyeH = 10 * sc * eyeBlink;
  ctx.fillStyle = colors.eyes || '#14120f';
  ctx.beginPath();
  ctx.roundRect(cx - 14 * sc, eyeY, eyeW, eyeH, 2 * sc);
  ctx.fill();
  ctx.beginPath();
  ctx.roundRect(cx + 6 * sc, eyeY, eyeW, eyeH, 2 * sc);
  ctx.fill();
  if (eyeBlink > 0.5) {
    ctx.fillStyle = 'rgba(255,255,255,0.75)';
    ctx.beginPath();
    ctx.arc(cx - 11 * sc, eyeY + 2.5 * sc, 2 * sc, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.arc(cx + 9 * sc, eyeY + 2.5 * sc, 2 * sc, 0, Math.PI * 2);
    ctx.fill();
  }

  ctx.restore();
}

// ─── Animation control ────────────────────────────────────────────────────────

function startAnim() {
  if (rafId !== null) return;
  lastTimestamp = performance.now();
  rafId = requestAnimationFrame(renderFrame);
}

function stopAnim() {
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
}

// Initialise hit-zone to the starting character position immediately so it is
// grabbable before the first animation frame fires.
updateHitZone();

startAnim();

document.addEventListener('visibilitychange', () => {
  if (document.hidden) stopAnim(); else startAnim();
});

// ─── Hit-zone drag + click ────────────────────────────────────────────────────
// Full 2D drag — user can move the companion anywhere on screen.
//
// Passthrough model: the overlay window keeps set_ignore_cursor_events(false) at
// all times so the OS always delivers mouse events to the WebView.  Transparent /
// non-interactive areas use CSS pointer-events:none so clicks fall through to the
// desktop at the WebView hit-test level — no window-level OS toggle is needed.
// This is more reliable than the toggle approach which broke on fast drags where
// the cursor left the hit-zone before mousemove arrived.

let _dragActive = false;
let _dragStartX = 0;
let _dragStartY = 0;
let _walkXAtDragStart = 0;
let _walkYAtDragStart = 0;

function _beginDrag(clientX, clientY) {
  _dragActive = true;
  _dragStartX = clientX;
  _dragStartY = clientY;
  _walkXAtDragStart = walkX;
  _walkYAtDragStart = walkY;
  hitZone.classList.add('dragging');
  walking = false;
  currentState = 'idle';
}

function _endDrag() {
  if (!_dragActive) return;
  _dragActive = false;
  hitZone.classList.remove('dragging');
  // Brief cooldown before auto-walk resumes so the companion stays where dropped
  _dragResumeAt = performance.now() + DRAG_RESUME_MS;
  // Show a friendly drop reaction
  const drops = ['🎯 Right here!', '📍 Perfect spot!', '👌 Done!'];
  showBubble(drops[Math.floor(Math.random() * drops.length)], 1500);
}

hitZone.addEventListener('mousedown', (e) => {
  e.preventDefault();
  _beginDrag(e.clientX, e.clientY);
});

window.addEventListener('mousemove', (e) => {
  if (!_dragActive) return;
  const dx = e.clientX - _dragStartX;
  const dy = e.clientY - _dragStartY;
  walkX = Math.max(CHAR_MARGIN, Math.min(W - CHAR_MARGIN, _walkXAtDragStart + dx));
  walkY = Math.max(COMPANION_TARGET_H + 10, Math.min(H - 10, _walkYAtDragStart + dy));
});

window.addEventListener('mouseup', _endDrag);

// Safety net: if the window loses focus mid-drag (e.g. Alt+Tab) restore passthrough
window.addEventListener('blur', () => { if (_dragActive) _endDrag(); });

// Touch drag support
hitZone.addEventListener('touchstart', (e) => {
  if (e.touches.length !== 1) return;
  e.preventDefault();
  const t = e.touches[0];
  _beginDrag(t.clientX, t.clientY);
}, { passive: false });

window.addEventListener('touchmove', (e) => {
  if (!_dragActive || e.touches.length !== 1) return;
  const t = e.touches[0];
  const dx = t.clientX - _dragStartX;
  const dy = t.clientY - _dragStartY;
  walkX = Math.max(CHAR_MARGIN, Math.min(W - CHAR_MARGIN, _walkXAtDragStart + dx));
  walkY = Math.max(COMPANION_TARGET_H + 10, Math.min(H - 10, _walkYAtDragStart + dy));
});

window.addEventListener('touchend', _endDrag);
window.addEventListener('touchcancel', _endDrag);

// ─── Single click: live pet reactions ────────────────────────────────────────

// ── Richer click reactions — cycles through all moods ────────────────────
const CLICK_REACTIONS = [
  { state: 'wave',    bubble: '👋 Hey there!',              spring: -8,  dur: 3000 },
  { state: 'happy',   bubble: '✨ You noticed me!',          spring: -14, dur: 3500 },
  { state: 'water',   bubble: '💧 Drink some water!',        spring: -6,  dur: 3000 },
  { state: 'stretch', bubble: '🧘 Time to stretch!',         spring: -5,  dur: 3000 },
  { state: 'happy',   bubble: '😄 Wohoo! Click again!',      spring: -12, dur: 2500 },
  { state: 'eyes',    bubble: '👁️ Look away from screen!',   spring: -4,  dur: 3200 },
  { state: 'sad',     bubble: '😴 Remember to rest!',        spring: -3,  dur: 3000 },
  { state: 'wave',    bubble: '🌟 Keep up the great work!',  spring: -9,  dur: 3000 },
  { state: 'happy',   bubble: '🎊 You\'re awesome!',         spring: -16, dur: 3500 },
  { state: 'water',   bubble: '☕ Coffee or water break?',   spring: -7,  dur: 3000 },
];
let _clickReactionIdx = 0;
// Track time of last click to detect rapid clicks (excitement boost)
let _lastClickTime = 0;

hitZone.addEventListener('click', () => {
  // Ignore click if the user just finished a drag (moved more than 5px)
  const movedX = Math.abs(walkX - _walkXAtDragStart);
  const movedY = Math.abs(walkY - _walkYAtDragStart);
  if (movedX > 5 || movedY > 5) return;

  const now = performance.now();
  const rapid = now - _lastClickTime < 600; // rapid double-ish click
  _lastClickTime = now;

  // Every 3rd click: surface live capture stats if capturing
  let reaction;
  if (_clickReactionIdx % 3 === 0 && _captureStatus.capturing) {
    reaction = {
      state: 'happy',
      bubble: `📊 ${_captureStatus.eventsSeen} events · ${_captureStatus.draftsSaved} drafts`,
      spring: -10,
      dur: 3500,
    };
  } else if (rapid) {
    // Rapid tapping = excited bounce
    reaction = { state: 'happy', bubble: '🎉 Yay!', spring: -18, dur: 2000 };
  } else {
    reaction = CLICK_REACTIONS[_clickReactionIdx % CLICK_REACTIONS.length];
  }
  _clickReactionIdx++;

  currentState = reaction.state;
  walking = false;
  springVY = reaction.spring;
  nextStateTimer = performance.now() + reaction.dur;
  showBubble(reaction.bubble, reaction.dur - 200);
  loadSprite(companion.id, reaction.state);
});

hitZone.addEventListener('dblclick', () => {
  currentState = 'happy';
  walking = false;
  springVY = -22;
  nextStateTimer = performance.now() + 4500;
  showBubble('🎉 Woohoo!! You found me! 🎊', 3000);
  loadSprite(companion.id, 'happy');
});

// ─── Reload companion when activated from main window ────────────────────────

async function reloadCompanion() {
  const next = await loadActiveCompanionFromIPC();
  if (next.id === companion.id) {
    // Same companion — but actions list might have changed (behavior editor).
    // Update in-place and refresh WALK_STATES without clearing sprites.
    if (JSON.stringify(next.actions) !== JSON.stringify(companion.actions)) {
      companion = next;
      WALK_STATES = getWalkStates();
    }
    return;
  }
  companion = next;
  // Refresh WALK_STATES for the new companion's action set.
  WALK_STATES = getWalkStates();
  // Clear sprite cache for old companion so stale frames don't bleed through
  for (const k of Object.keys(spriteCache)) {
    if (!k.startsWith(companion.id + ':')) delete spriteCache[k];
  }
  for (const k of Object.keys(spriteLoading)) {
    if (!k.startsWith(companion.id + ':')) delete spriteLoading[k];
  }
  // Pre-warm all common actions so transitions are instant
  prewarmSprites(companion.id);
  pickNextState();
}

// Initial async load — replace the default Grace with the persisted companion.
loadActiveCompanionFromIPC().then((state) => {
  companion = state;
  // Refresh WALK_STATES to reflect this companion's action list.
  WALK_STATES = getWalkStates();
  // Pre-warm all common sprites so the companion appears without a procedural flash.
  prewarmSprites(companion.id);
});

// Poll for companion changes every 2 s.  localStorage events do not fire
// cross-window inside Tauri's isolated WebViews, so polling the shared IPC
// file is the only cross-window signalling mechanism available.
setInterval(reloadCompanion, 2000);

// ─── Free Roam toggle button ──────────────────────────────────────────────────

syncRoamBtn(); // set initial visual state

const roamBtn = document.getElementById('roam-btn');
if (roamBtn) {
  roamBtn.addEventListener('click', (e) => {
    e.stopPropagation(); // don't trigger hit-zone events
    freeRoam = !freeRoam;
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(ROAM_STORAGE_KEY, String(freeRoam));
    }
    syncRoamBtn();

    if (freeRoam) {
      // Kick off roam from current position — pick a first waypoint
      pickRoamTarget();
      // Reset bottom-walk state so the companion doesn't snap to the edge
      pauseTimer = 0;
      walking = false;
      showBubble('🗺️ Free Roam on! I\'ll explore.', 2500);
    } else {
      // Return companion to bottom of screen gracefully
      walkY = H;
      _roamPauseUntil = 0;
      walking = false;
      currentState = 'idle';
      nextStateTimer = performance.now() + 2000;
      showBubble('🌐 Back to the bottom!', 2000);
    }
  });
}
