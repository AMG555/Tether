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
 * All Tauri IPC calls must go through api.js (architecture rule).
 */

import { getCompanionClip } from './api.js';

// ─── Constants ────────────────────────────────────────────────────────────────

const WALK_SPEED_PX   = 2.2;      // pixels per frame (faster, more lively)
const WALK_PAUSE_MS   = 4000;     // pause at edge before turning
const IDLE_PAUSE_CHANCE = 0.002;  // per-frame chance to pause mid-walk
const SPRITE_FPS      = 12;       // WebP sprite playback rate
const CHAR_SCALE      = 1.55;     // global scale for procedural character
const CHAR_MARGIN     = 60;       // px from each horizontal edge — walk boundary
const CHAR_WIDTH      = 80;       // approximate half-width for hit-testing
const CHAR_HEIGHT     = 220;      // approximate full height for hit-testing
const COMPANION_TARGET_H = 220;   // render height in logical CSS pixels

// ─── Companion data ───────────────────────────────────────────────────────────

const DEFAULT_COMPANION = {
  id: 'grace',
  name: 'Grace',
  colors: { body: '#2563eb', accent: '#c2331a', eyes: '#14120f' },
};

function loadActiveCompanion() {
  try {
    const raw = localStorage.getItem('tether-active-companion');
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && parsed.id) return parsed;
    }
  } catch { /* ignore */ }
  return DEFAULT_COMPANION;
}

let companion = loadActiveCompanion();

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
  walkX = Math.max(CHAR_MARGIN, Math.min(W - CHAR_MARGIN, walkX));
  walkY = Math.max(CHAR_HEIGHT + 10, Math.min(H - 10, walkY));
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
  try {
    const raw = localStorage.getItem('tether-active-actions');
    if (raw) {
      const ids = JSON.parse(raw);
      if (Array.isArray(ids) && ids.length > 0) return ids;
    }
  } catch { /* ignore */ }
  return ['idle', 'idle', 'walk', 'walk', 'wave', 'happy', 'water', 'stretch'];
}

function getActionQuotes() {
  try {
    const raw = localStorage.getItem('tether-action-quotes');
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
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

function showBubble(text, durationMs = 3000) {
  bubbleText = text;
  bubbleAlpha = 1;
  bubbleTimer = performance.now() + durationMs;
}

// ─── WebP sprite cache ────────────────────────────────────────────────────────

/** @type {Record<string, HTMLImageElement | null>} */
const spriteCache = {};
/** @type {Record<string, boolean>} */
const spriteLoading = {};

async function loadSprite(companionId, state) {
  const key = `${companionId}:${state}`;
  if (key in spriteCache) return spriteCache[key];
  if (spriteLoading[key]) return undefined;
  spriteLoading[key] = true;
  try {
    const dataUrl = await getCompanionClip(companionId, state);
    if (dataUrl) {
      const img = new Image();
      img.src = dataUrl;
      await new Promise((res) => { img.onload = res; img.onerror = res; });
      if (img.naturalWidth > 0) {
        spriteCache[key] = img;
        return img;
      }
    }
  } catch { /* fall through */ }
  delete spriteLoading[key];
  return null;
}

// Pre-warm the idle and walk sprites immediately on load
loadSprite(companion.id, 'idle');
loadSprite(companion.id, 'walk');

// ─── State helpers ────────────────────────────────────────────────────────────

const DEFAULT_QUOTES = {
  idle: '', walk: '', wave: '👋 Hey there!', water: '💧 Stay hydrated!',
  happy: '✨ Looking good!', stretch: '🧘 Stretch time!', sad: '😴 Rest your eyes…',
};

function pickNextState() {
  WALK_STATES = getWalkStates();
  if (WALK_STATES.length === 0) WALK_STATES = ['idle'];

  const idx = Math.floor(Math.random() * WALK_STATES.length);
  currentState = WALK_STATES[idx];
  walking = currentState === 'walk';
  nextStateTimer = performance.now() + (5000 + Math.random() * 7000);
  loadSprite(companion.id, currentState);

  const quotes = getActionQuotes();
  const text = quotes[currentState] || DEFAULT_QUOTES[currentState] || '';
  if (text) showBubble(text, 2800);
}

pickNextState();

// ─── Render loop ──────────────────────────────────────────────────────────────

let lastTimestamp = 0;
let spriteFrame = 0;
let spriteFrameTimer = 0;

function renderFrame(timestamp) {
  rafId = null;
  const dt = Math.min(timestamp - lastTimestamp, 50);
  lastTimestamp = timestamp;
  animTime += dt * 0.001;

  // Sprite frame advance
  spriteFrameTimer += dt;
  if (spriteFrameTimer > 1000 / SPRITE_FPS) {
    spriteFrame++;
    spriteFrameTimer = 0;
  }

  // State transition
  if (timestamp > nextStateTimer) pickNextState();

  // Walk position — X axis walk, Y stays where user dragged it (or bottom)
  if (walking) {
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
      } else if (walkX < CHAR_MARGIN) {
        walkX = CHAR_MARGIN;
        walkDir = 1;
        walking = false;
        pauseTimer = timestamp + WALK_PAUSE_MS;
        currentState = 'idle';
      }
    }
  } else if (pauseTimer > 0 && timestamp > pauseTimer) {
    pauseTimer = 0;
    walking = true;
    currentState = 'walk';
    nextStateTimer = performance.now() + 9000;
  }

  // Spring physics
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
  const pad = 10;
  ctx.font = 'bold 13px -apple-system, Segoe UI, sans-serif';
  const tw = ctx.measureText(bubbleText).width;
  const bw = tw + pad * 2;
  const bh = 32;

  ctx.save();
  ctx.globalAlpha = Math.min(bubbleAlpha, 0.95);

  ctx.fillStyle = '#fff';
  ctx.strokeStyle = 'rgba(0,0,0,0.12)';
  ctx.lineWidth = 1;
  const rx = bx - bw / 2;
  const ry = by - bh;
  ctx.beginPath();
  ctx.roundRect(rx, ry, bw, bh, 10);
  ctx.fill();
  ctx.stroke();

  ctx.fillStyle = '#fff';
  ctx.beginPath();
  ctx.moveTo(bx - 7, by);
  ctx.lineTo(bx, by + 10);
  ctx.lineTo(bx + 7, by);
  ctx.closePath();
  ctx.fill();

  ctx.fillStyle = '#1f2328';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText(bubbleText, bx, ry + bh / 2);

  ctx.restore();
}

function drawCompanion() {
  const spriteKey = `${companion.id}:${currentState}`;
  const img = spriteCache[spriteKey];
  const shouldFlip = walking && walkDir === -1;

  if (img) {
    const iw = img.naturalWidth;
    const ih = img.naturalHeight;
    if (iw === 0 || ih === 0) {
      drawProceduralCompanion(shouldFlip);
      return;
    }
    const scale = Math.min(COMPANION_TARGET_H / ih, (W * 0.5) / iw);
    const dw = iw * scale;
    const dh = ih * scale;
    const dx = walkX - dw / 2;
    const dy = walkY - dh + springY;    // feet at walkY
    ctx.save();
    if (shouldFlip) { ctx.translate(walkX * 2, 0); ctx.scale(-1, 1); }
    ctx.drawImage(img, dx, dy, dw, dh);
    ctx.restore();
  } else {
    if (!spriteLoading[spriteKey]) {
      loadSprite(companion.id, currentState);
    }
    drawProceduralCompanion(shouldFlip);
  }
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
  const leftArmRot = currentState === 'stretch'
    ? -1.4 + Math.sin(animTime * 1.5) * 0.15
    : currentState === 'wave' ? -1.15 + waveAngle
    : 0.18 - walkPhase * 0.28;
  ctx.rotate(leftArmRot);
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(-9 * sc, 0, 9 * sc, 26 * sc, 4 * sc);
  ctx.fill();
  ctx.restore();

  ctx.save();
  ctx.translate(cx + bodyW / 2, baseY - legH - bodyH * 0.75 - bob);
  const rightArmRot = currentState === 'stretch'
    ? 1.4 - Math.sin(animTime * 1.5) * 0.15
    : currentState === 'wave' ? 1.1 - waveAngle
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

let _dragActive = false;
let _dragStartX = 0;
let _dragStartY = 0;
let _walkXAtDragStart = 0;
let _walkYAtDragStart = 0;

hitZone.addEventListener('mousedown', (e) => {
  e.preventDefault();
  _dragActive = true;
  _dragStartX = e.clientX;
  _dragStartY = e.clientY;
  _walkXAtDragStart = walkX;
  _walkYAtDragStart = walkY;
  hitZone.classList.add('dragging');
  walking = false;
  currentState = 'idle';
});

window.addEventListener('mousemove', (e) => {
  if (!_dragActive) return;
  const dx = e.clientX - _dragStartX;
  const dy = e.clientY - _dragStartY;
  walkX = Math.max(CHAR_MARGIN, Math.min(W - CHAR_MARGIN, _walkXAtDragStart + dx));
  // Keep the companion fully visible vertically (feet must be on screen)
  walkY = Math.max(COMPANION_TARGET_H + 10, Math.min(H - 10, _walkYAtDragStart + dy));
});

window.addEventListener('mouseup', () => {
  if (!_dragActive) return;
  _dragActive = false;
  hitZone.classList.remove('dragging');
});

// Touch drag support
hitZone.addEventListener('touchstart', (e) => {
  if (e.touches.length !== 1) return;
  e.preventDefault();
  const t = e.touches[0];
  _dragActive = true;
  _dragStartX = t.clientX;
  _dragStartY = t.clientY;
  _walkXAtDragStart = walkX;
  _walkYAtDragStart = walkY;
  hitZone.classList.add('dragging');
  walking = false;
  currentState = 'idle';
}, { passive: false });

window.addEventListener('touchmove', (e) => {
  if (!_dragActive || e.touches.length !== 1) return;
  const t = e.touches[0];
  const dx = t.clientX - _dragStartX;
  const dy = t.clientY - _dragStartY;
  walkX = Math.max(CHAR_MARGIN, Math.min(W - CHAR_MARGIN, _walkXAtDragStart + dx));
  walkY = Math.max(COMPANION_TARGET_H + 10, Math.min(H - 10, _walkYAtDragStart + dy));
});

window.addEventListener('touchend', () => {
  if (!_dragActive) return;
  _dragActive = false;
  hitZone.classList.remove('dragging');
});

// ─── Single click: live pet reactions ────────────────────────────────────────

const CLICK_REACTIONS = [
  { state: 'wave',    bubble: '👋 Hey there!',       spring: -8,  dur: 3000 },
  { state: 'happy',   bubble: '✨ You noticed me!',   spring: -14, dur: 3500 },
  { state: 'water',   bubble: '💧 Drink some water!', spring: -6,  dur: 3000 },
  { state: 'stretch', bubble: '🧘 Time to stretch!',  spring: -5,  dur: 3000 },
  { state: 'happy',   bubble: '😄 Click me again!',   spring: -12, dur: 2500 },
];
let _clickReactionIdx = 0;

hitZone.addEventListener('click', () => {
  // Ignore click if the user just finished a drag (moved more than 5px)
  const movedX = Math.abs(walkX - _walkXAtDragStart);
  const movedY = Math.abs(walkY - _walkYAtDragStart);
  if (movedX > 5 || movedY > 5) return;
  const reaction = CLICK_REACTIONS[_clickReactionIdx % CLICK_REACTIONS.length];
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
  springVY = -20;
  nextStateTimer = performance.now() + 4000;
  showBubble('🎉 Woohoo!!', 2500);
  loadSprite(companion.id, 'happy');
});

// ─── Reload companion when activated from main window ────────────────────────

function reloadCompanion() {
  companion = loadActiveCompanion();
  // Clear sprite cache for old companion
  Object.keys(spriteCache).forEach((k) => {
    if (!k.startsWith(companion.id + ':')) delete spriteCache[k];
  });
  Object.keys(spriteLoading).forEach((k) => {
    if (!k.startsWith(companion.id + ':')) delete spriteLoading[k];
  });
  // Pre-warm idle + walk so the real image appears without robot flash
  loadSprite(companion.id, 'idle');
  loadSprite(companion.id, 'walk');
  pickNextState();
}

window.addEventListener('companion-activated', reloadCompanion);

window.addEventListener('storage', (e) => {
  if (e.key === 'tether-active-companion') reloadCompanion();
});
