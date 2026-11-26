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

import { getCompanionClip, getActiveCompanion, setActiveCompanion, getCaptureStatus, setCompanionPassthrough, setHitZoneRegion } from './api.js';
import { createBrain } from './companion-brain.js';

// Global error handlers: guarantee the companion never crashes or freezes
if (typeof window !== 'undefined') {
  window.addEventListener('error', (e) => {
    console.warn('[Tether Overlay Safe Guard]', e.message || e.error);
  });
  window.addEventListener('unhandledrejection', (e) => {
    console.warn('[Tether Overlay Safe Guard]', e.reason);
  });
}

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
// instead of only patrolling the bottom edge. Synced across WebViews via IPC
// and localStorage fallback.

const ROAM_STORAGE_KEY   = 'tether_companion_free_roam';
const ROAM_SPEED_PX      = 2.0;   // lively roam speed
const ROAM_ARRIVE_DIST   = 35;    // px — arrival threshold
const ROAM_PAUSE_MIN_MS  = 3000;  // pause at each waypoint
const ROAM_PAUSE_MAX_MS  = 6500;  // pause at each waypoint
const ROAM_MARGIN_X      = 70;    // px from horizontal edges
const ROAM_MARGIN_TOP    = CHAR_HEIGHT + 56; // px from top (clear head + speech bubble)
const ROAM_MARGIN_BOT    = 10;    // px from bottom

let freeRoam = typeof localStorage !== 'undefined'
  && localStorage.getItem(ROAM_STORAGE_KEY) === 'true';

// Current roam waypoint the companion is walking toward
let _roamTargetX = 400;
let _roamTargetY = 600;
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

// Local compact window dimensions (360×310 CSS px) + full desktop screen bounds.
// Walking moves the compact window across screenW×screenH via DWM SetWindowPos —
// never full-screen, so browsers never go blank from occlusion and no GDI
// SetWindowRgn clipping is needed (zero white-box flashing).
let W = window.innerWidth || 360;
let H = window.innerHeight || 310;
let screenW = (typeof window !== 'undefined' && window.screen?.width) || 1920;
let screenH = (typeof window !== 'undefined' && window.screen?.height) || 1080;

function refreshScreenBounds() {
  if (typeof window !== 'undefined' && window.screen) {
    if (window.screen.width > 0) screenW = window.screen.width;
    if (window.screen.height > 0) screenH = window.screen.height;
  }
}

let _appliedDpr = 1;
function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  const cw = window.innerWidth || 360;
  const ch = window.innerHeight || 310;
  const w = Math.round(cw * dpr);
  const h = Math.round(ch * dpr);
  W = cw;
  H = ch;
  refreshScreenBounds();
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width  = w;
    canvas.height = h;
    ctx.scale(dpr, dpr);
    _appliedDpr = dpr;
  }
}
resizeCanvas();

function getScreenMargins() {
  const minX = Math.max(CHAR_MARGIN, Math.round(W / 2));
  const maxX = Math.max(minX + 40, screenW - minX);
  const minY = Math.max(ROAM_MARGIN_TOP, H - 10);
  const maxY = Math.max(minY, screenH - 8);
  return { minX, maxX, minY, maxY };
}

window.addEventListener('resize', () => {
  resizeCanvas();
  const { minX, maxX, minY, maxY } = getScreenMargins();
  walkX = Math.max(minX, Math.min(maxX, walkX));
  walkY = freeRoam ? Math.max(minY, Math.min(maxY, walkY)) : maxY;
  if (freeRoam) pickRoamTarget();
  updateHitZone(true);
});

// Drag state — declared before updateHitZone() to avoid TDZ errors on initial boot.
let _dragActive = false;
let _dragMoved = false;
let _dragStartX = 0;
let _dragStartY = 0;
let _walkXAtDragStart = 0;
let _walkYAtDragStart = 0;

// Keep #hit-zone aligned over the character inside the compact window and move
// the OS window smoothly to follow (walkX, walkY) across the desktop.
let _lastWinX = -9999, _lastWinY = -9999;
let _regionPending = false;

function updateHitZone(force = false) {
  const totalW = CHAR_WIDTH * 2; // 160px interactive body width
  const totalH = CHAR_HEIGHT;    // 220px interactive body height
  const localX = Math.round(W / 2 - totalW / 2);
  const localY = Math.round((H - 10) - totalH + springY);

  hitZone.style.left   = `${localX}px`;
  hitZone.style.top    = `${localY}px`;
  hitZone.style.bottom = '';
  hitZone.style.width  = `${totalW}px`;
  hitZone.style.height = `${totalH}px`;

  const winLeft = Math.round(walkX - W / 2);
  const winTop  = Math.round(walkY - (H - 10));
  const moved = Math.abs(winLeft - _lastWinX) >= 2 || Math.abs(winTop - _lastWinY) >= 2;

  if (force || moved) {
    _lastWinX = winLeft;
    _lastWinY = winTop;
    if (!_regionPending) {
      _regionPending = true;
      Promise.resolve().then(() => {
        _regionPending = false;
        const dpr = window.devicePixelRatio || 1;
        void setHitZoneRegion(_lastWinX, _lastWinY, totalW, totalH, dpr);
      });
    }
  }
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
let walkX = screenW / 2;   // desktop X (centred on primary screen)
let walkY = screenH - 8;   // desktop feet Y (above auto-hide taskbar edge)
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
  updateHitZone(true);
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
  refreshScreenBounds();
  const { minX, maxX, minY, maxY } = getScreenMargins();
  // Prefer a waypoint clearly away from the current spot so Free Roam visibly
  // leaves the bottom edge immediately instead of pausing right where it started.
  let tx = minX + Math.random() * (maxX - minX);
  let ty = minY + Math.random() * (maxY - minY);
  for (let i = 0; i < 4 && Math.hypot(tx - walkX, ty - walkY) < 160; i++) {
    tx = minX + Math.random() * (maxX - minX);
    ty = minY + Math.random() * (maxY - minY);
  }
  _roamTargetX = tx;
  _roamTargetY = ty;
  _roamPauseUntil = 0; // start walking immediately
}

/** Step companion one frame toward current roam target. Returns true while still walking. */
function stepRoam(timestamp) {
  // Waiting at a waypoint
  if (_roamPauseUntil > 0) {
    if (timestamp < _roamPauseUntil) return false; // still pausing
    _roamPauseUntil = 0;
    pickRoamTarget();
    walking = true;
    currentState = 'walk';
    loadSprite(companion.id, 'walk');
    return true;
  }

  const dx = _roamTargetX - walkX;
  const dy = _roamTargetY - walkY;
  const dist = Math.sqrt(dx * dx + dy * dy);

  if (dist < ROAM_ARRIVE_DIST) {
    // Arrived at destination waypoint! Pause here and perform a resting action
    const pauseMs = ROAM_PAUSE_MIN_MS + Math.random() * (ROAM_PAUSE_MAX_MS - ROAM_PAUSE_MIN_MS);
    _roamPauseUntil = timestamp + pauseMs;
    walking = false;

    // Pick an expressive resting action while stopped
    const restActions = ['idle', 'idle', 'wave', 'stretch', 'water', 'happy', 'eyes'];
    const available = companion.actions && companion.actions.length > 0
      ? restActions.filter((a) => companion.actions.includes(a))
      : restActions;
    currentState = available[Math.floor(Math.random() * available.length)] || 'idle';
    loadSprite(companion.id, currentState);

    const quotes = getActionQuotes();
    const text = quotes[currentState]
      || brain.messageFor(currentState)
      || DEFAULT_QUOTES[currentState]
      || '';
    if (text) showBubble(text, Math.min(pauseMs - 200, 3200));
    return false;
  }

  // Move toward target — normalise direction and scale by speed
  walkX += (dx / dist) * ROAM_SPEED_PX;
  walkY += (dy / dist) * ROAM_SPEED_PX;
  // Keep inside safe desktop bounds
  const { minX, maxX, minY, maxY } = getScreenMargins();
  walkX = Math.max(minX, Math.min(maxX, walkX));
  walkY = Math.max(minY, Math.min(maxY, walkY));
  // Update face direction based on horizontal movement
  walkDir = dx >= 0 ? 1 : -1;
  return true; // still walking
}

async function toggleFreeRoam() {
  freeRoam = !freeRoam;
  if (typeof localStorage !== 'undefined') {
    try { localStorage.setItem(ROAM_STORAGE_KEY, String(freeRoam)); } catch {}
  }

  if (freeRoam) {
    pickRoamTarget();
    pauseTimer = 0;
    walking = true;
    currentState = 'walk';
    loadSprite(companion.id, 'walk');
    showBubble('🗺️ Free Roam on! Exploring everywhere.', 2500);
  } else {
    walkY = getScreenMargins().maxY;
    _roamPauseUntil = 0;
    walking = false;
    currentState = 'idle';
    loadSprite(companion.id, 'idle');
    nextStateTimer = performance.now() + 2000;
    showBubble('🌐 Back to the bottom!', 2000);
  }

  try {
    const active = await getActiveCompanion();
    if (active) {
      active.freeRoam = freeRoam;
      await setActiveCompanion(active);
    }
  } catch { /* ignore */ }
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
  el.style.cssText = 'position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;pointer-events:none;overflow:hidden;';
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
      await new Promise((res) => {
        let done = false;
        const finish = () => { if (!done) { done = true; res(); } };
        img.onload = () => requestAnimationFrame(finish);
        img.onerror = finish;
        setTimeout(finish, 1500);
      });
      // Accept image if it has nonzero dimensions OR a nonzero src (animated WebP
      // with decoded-frame delay can still report 0×0 briefly after onload).
      if (img.naturalWidth > 0 || img.src.length > 0) {
        spriteCache[key] = img;
        delete spriteLoading[key];
        return img;
      }
      // Image appears genuinely broken — remove the orphan element safely.
      if (img.parentNode) img.parentNode.removeChild(img);
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
let lastRenderTime = 0;
const TARGET_FRAME_MS = 1000 / 30; // 30 FPS cap for idle companion animation saves 50% CPU/GPU and drops RAM footprint

function renderFrame(timestamp) {
  rafId = null;
  try {
    if (lastRenderTime > 0 && timestamp - lastRenderTime < TARGET_FRAME_MS) {
      rafId = requestAnimationFrame(renderFrame);
      return;
    }
    const dt = Math.min(timestamp - (lastRenderTime || timestamp), 50);
    lastTimestamp = timestamp;
    lastRenderTime = timestamp;
    animTime += dt * 0.001;

    // State transition — standard bottom patrol mode only (free-roam manages its own waypoint actions)
    if (!freeRoam && timestamp > nextStateTimer && !_dragActive && pauseTimer === 0) {
      pickNextState();
    }

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
        if (Math.random() < 0.008) springVY = -4;
      }
    } else {
      // ── Standard mode: patrol along the bottom edge of the desktop ───────────
      if (walking && walkAllowed) {
        if (Math.random() < IDLE_PAUSE_CHANCE) {
          walking = false;
          currentState = 'idle';
          nextStateTimer = performance.now() + 3000;
        } else {
          const { minX, maxX } = getScreenMargins();
          walkX += walkDir * WALK_SPEED_PX;
          if (walkX > maxX) {
            walkX = maxX;
            walkDir = -1;
            walking = false;
            pauseTimer = timestamp + WALK_PAUSE_MS;
            currentState = 'idle';
            nextStateTimer = performance.now() + WALK_PAUSE_MS + 1000;
          } else if (walkX < minX) {
            walkX = minX;
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
  } catch {
    // Non-fatal: safeguard render loop so canvas never freezes permanently
  }

  rafId = requestAnimationFrame(renderFrame);
}

function drawBubble() {
  const localX = W / 2;
  const localY = H - 10;
  const pad = 12;
  const maxTextW = Math.max(120, W - 40);
  ctx.font = 'bold 13px -apple-system, Segoe UI, sans-serif';

  // Wrap long captions cleanly onto up to 2 lines so nothing is ever trimmed.
  const lines = [];
  if (ctx.measureText(bubbleText).width <= maxTextW) {
    lines.push(bubbleText);
  } else {
    const words = bubbleText.split(/\s+/);
    let line = '';
    for (const word of words) {
      const candidate = line ? `${line} ${word}` : word;
      if (ctx.measureText(candidate).width <= maxTextW || !line) {
        line = candidate;
      } else {
        lines.push(line);
        line = word;
        if (lines.length === 1) break;
      }
    }
    if (line) {
      const rest = words.slice(lines[0] ? lines[0].split(/\s+/).length : 0).join(' ');
      let second = lines.length === 1 && rest ? rest : line;
      while (second.length > 4 && ctx.measureText(second).width > maxTextW) {
        second = second.slice(0, -2) + '…';
      }
      lines.push(second);
    }
  }

  const maxLineW = Math.max(...lines.map((l) => ctx.measureText(l).width));
  const bw = Math.min(W - 16, maxLineW + pad * 2);
  const lineH = 16;
  const bh = lines.length > 1 ? 46 : 32;

  // Position bubble cleanly ABOVE the character's head (never overlapping the face).
  const by = Math.max(bh + 8, localY - COMPANION_TARGET_H - 12 + springY);

  ctx.save();
  ctx.globalAlpha = Math.min(bubbleAlpha, 0.96);

  const clampedBx = Math.max(bw / 2 + 8, Math.min(W - bw / 2 - 8, localX));
  const rx = clampedBx - bw / 2;
  const ry = by - bh;

  // Drop shadow
  ctx.shadowColor = 'rgba(0,0,0,0.14)';
  ctx.shadowBlur = 8;
  ctx.shadowOffsetY = 2;

  ctx.fillStyle = '#fff';
  ctx.strokeStyle = 'rgba(0,0,0,0.12)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(rx, ry, bw, bh, 10);
  ctx.fill();
  ctx.stroke();

  ctx.shadowColor = 'transparent';
  ctx.shadowBlur = 0;
  ctx.shadowOffsetY = 0;

  // Tail — points down from bubble toward top of companion's head
  const tailX = Math.max(rx + 12, Math.min(rx + bw - 12, localX));
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
  if (lines.length === 1) {
    ctx.fillText(lines[0], clampedBx, ry + bh / 2);
  } else {
    ctx.fillText(lines[0], clampedBx, ry + bh / 2 - lineH / 2);
    ctx.fillText(lines[1], clampedBx, ry + bh / 2 + lineH / 2);
  }

  ctx.restore();
}

// Draw the live status indicator (e.g. 🔴 when capturing) above the companion
function drawStatusIndicator() {
  if (!_statusIndicator) return;
  const localX = W / 2;
  const localY = H - 10;
  ctx.save();
  ctx.font = '14px -apple-system, Segoe UI, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.globalAlpha = 0.9;
  ctx.fillText(_statusIndicator, localX + CHAR_WIDTH * 0.6, localY - COMPANION_TARGET_H + springY - 8);
  ctx.restore();
}

const spriteImgEl = document.getElementById('companion-sprite');
let _lastRenderedSrc = '';
let _lastRenderedFlip = null;

/** Render the loaded animated WebP directly via DOM <img> element.
 *
 * Chromium/WebView2 canvas drawImage() captures only static frame 0
 * of an animated WebP (poster frame), making canvas sprites look frozen.
 * Displaying the <img> in the DOM enables native hardware-accelerated
 * WebP frame ticking at full 60fps so the companion is truly live.
 */
function renderSprite(img, flip) {
  if (!spriteImgEl) return;
  const localX = W / 2;
  const localY = H - 10;
  const scale = Math.min(
    COMPANION_TARGET_H / (img.naturalHeight || COMPANION_TARGET_H),
    (W * 0.72) / (img.naturalWidth || 160)
  );
  const dw = Math.round((img.naturalWidth || 160) * scale);
  const dh = Math.round((img.naturalHeight || COMPANION_TARGET_H) * scale);
  const dx = Math.round(localX - dw / 2);
  const dy = Math.round(localY - dh + springY);

  if (spriteImgEl.style.display !== 'block') {
    spriteImgEl.style.display = 'block';
  }

  // Only assign src when changed so the browser doesn't reset the animation clock!
  if (_lastRenderedSrc !== img.src) {
    _lastRenderedSrc = img.src;
    spriteImgEl.src = img.src;
  }

  spriteImgEl.style.width = `${dw}px`;
  spriteImgEl.style.height = `${dh}px`;
  spriteImgEl.style.left = `${dx}px`;
  spriteImgEl.style.top = `${dy}px`;

  if (_lastRenderedFlip !== flip) {
    _lastRenderedFlip = flip;
    spriteImgEl.style.transform = flip ? 'scaleX(-1)' : 'none';
  }
}

function hideSprite() {
  if (spriteImgEl && spriteImgEl.style.display !== 'none') {
    spriteImgEl.style.display = 'none';
  }
}

function drawCompanion() {
  const spriteKey = `${companion.id}:${currentState}`;

  // Flip logic: walk WebP sprites face right; mirror when walking left
  const shouldFlip = currentState === 'walk' && walkDir === -1;

  const img = spriteCache[spriteKey];

  if (img) {
    renderSprite(img, shouldFlip);
    drawStatusIndicator();
    return;
  }

  if (img === undefined) {
    loadSprite(companion.id, currentState);
  }

  // Bridge: use idle sprite while action sprite is loading
  const idleKey = `${companion.id}:idle`;
  const idleImg = spriteCache[idleKey];
  if (idleImg) {
    renderSprite(idleImg, false);
    drawStatusIndicator();
    return;
  }

  // Last resort: procedural animated character
  hideSprite();
  drawProceduralCompanion(shouldFlip);
  drawStatusIndicator();
}

function drawProceduralCompanion(flip) {
  const colors = companion.colors || DEFAULT_COMPANION.colors;
  const cx = W / 2;
  // Feet at bottom anchor of compact window, spring offset shifts entire body
  const baseY = H - 10;
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
updateHitZone(true);

startAnim();

document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    stopAnim();
  } else {
    updateHitZone(true);
    startAnim();
  }
});

// ─── Hit-zone drag + click ────────────────────────────────────────────────────
// Full 2D drag — user can move the companion anywhere on screen.
// On Windows, SetWindowRgn restricts the window shape to the hit-zone rect so
// everything outside it stays 100% click-through while the character remains
// clickable and draggable inside the region.

function _beginDrag(clientX, clientY) {
  _dragActive = true;
  _dragMoved = false;
  _dragStartX = clientX;
  _dragStartY = clientY;
  _walkXAtDragStart = walkX;
  _walkYAtDragStart = walkY;
  hitZone.classList.add('dragging');
  walking = false;
  currentState = 'idle';
  // Ensure OS cursor passthrough is disabled during drag
  void setCompanionPassthrough(false);
}

function _endDrag() {
  if (!_dragActive) return;
  _dragActive = false;
  hitZone.classList.remove('dragging');
  // Brief cooldown before auto-walk resumes so the companion stays where dropped
  _dragResumeAt = performance.now() + DRAG_RESUME_MS;

  // Release Rust DRAG_LOCK so cursor hover tracker resumes immediately
  void setCompanionPassthrough(true);
  updateHitZone(true);

  if (_dragMoved) {
    // Show a friendly drop reaction only if actually moved
    const drops = ['🎯 Right here!', '📍 Perfect spot!', '👌 Done!'];
    showBubble(drops[Math.floor(Math.random() * drops.length)], 1500);
  }
}

hitZone.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return; // only left click initiates drag
  e.preventDefault();
  _beginDrag(e.screenX ?? e.clientX, e.screenY ?? e.clientY);
});

window.addEventListener('mousemove', (e) => {
  if (!_dragActive) return;
  const sx = e.screenX ?? e.clientX;
  const sy = e.screenY ?? e.clientY;
  const dx = sx - _dragStartX;
  const dy = sy - _dragStartY;
  if (!_dragMoved && (Math.abs(dx) > 3 || Math.abs(dy) > 3)) {
    _dragMoved = true;
  }
  if (!_dragMoved) return;

  const { minX, maxX, minY, maxY } = getScreenMargins();
  walkX = Math.max(minX, Math.min(maxX, _walkXAtDragStart + dx));
  walkY = Math.max(minY, Math.min(maxY, _walkYAtDragStart + dy));
});

window.addEventListener('mouseup', _endDrag);

// Safety net: if the window loses focus mid-drag (e.g. Alt+Tab) restore passthrough
window.addEventListener('blur', () => { if (_dragActive) _endDrag(); });

// Touch drag support
hitZone.addEventListener('touchstart', (e) => {
  if (e.touches.length !== 1) return;
  e.preventDefault();
  const t = e.touches[0];
  _beginDrag(t.screenX ?? t.clientX, t.screenY ?? t.clientY);
}, { passive: false });

window.addEventListener('touchmove', (e) => {
  if (!_dragActive || e.touches.length !== 1) return;
  const t = e.touches[0];
  const sx = t.screenX ?? t.clientX;
  const sy = t.screenY ?? t.clientY;
  const dx = sx - _dragStartX;
  const dy = sy - _dragStartY;
  if (!_dragMoved && (Math.abs(dx) > 3 || Math.abs(dy) > 3)) {
    _dragMoved = true;
  }
  if (!_dragMoved) return;

  const { minX, maxX, minY, maxY } = getScreenMargins();
  walkX = Math.max(minX, Math.min(maxX, _walkXAtDragStart + dx));
  walkY = Math.max(minY, Math.min(maxY, _walkYAtDragStart + dy));
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
  // If user actually dragged the character, do not fire click reactions
  if (_dragMoved) return;

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
  if (_dragMoved) return;
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

  // Sync freeRoam mode if changed from another window
  if (typeof next.freeRoam === 'boolean' && next.freeRoam !== freeRoam) {
    freeRoam = next.freeRoam;
    if (typeof localStorage !== 'undefined') {
      try { localStorage.setItem(ROAM_STORAGE_KEY, String(freeRoam)); } catch {}
    }
    if (freeRoam) {
      pickRoamTarget();
      walking = true;
      currentState = 'walk';
      loadSprite(companion.id, 'walk');
      _roamPauseUntil = 0;
    } else {
      walkY = getScreenMargins().maxY;
      _roamPauseUntil = 0;
      walking = false;
      currentState = 'idle';
      loadSprite(companion.id, 'idle');
      nextStateTimer = performance.now() + 2000;
    }
  }

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
  _lastRenderedSrc = '';
  // Pre-warm all common actions so transitions are instant
  prewarmSprites(companion.id);
  if (freeRoam) {
    pauseTimer = 0;
    pickRoamTarget();
    walking = true;
    currentState = 'walk';
    loadSprite(companion.id, 'walk');
  } else {
    pickNextState();
  }
}

// Initial async load — replace the default Grace with the persisted companion.
loadActiveCompanionFromIPC().then((state) => {
  companion = state;
  if (typeof state.freeRoam === 'boolean') {
    freeRoam = state.freeRoam;
    if (typeof localStorage !== 'undefined') {
      try { localStorage.setItem(ROAM_STORAGE_KEY, String(freeRoam)); } catch {}
    }
  }
  if (freeRoam) {
    pauseTimer = 0;
    pickRoamTarget();
    walking = true;
    currentState = 'walk';
    loadSprite(companion.id, 'walk');
  }
  // Refresh WALK_STATES to reflect this companion's action list.
  WALK_STATES = getWalkStates();
  // Pre-warm all common sprites so the companion appears without a procedural flash.
  prewarmSprites(companion.id);
});

// Poll for companion changes every 2 s.  localStorage events do not fire
// cross-window inside Tauri's isolated WebViews, so polling the shared IPC
// file is the only cross-window signalling mechanism available.
setInterval(reloadCompanion, 2000);

// ─── Free Roam toggle interactions ──────────────────────────────────────────

// Right-click on companion toggles Free Roam
hitZone.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  toggleFreeRoam();
});
