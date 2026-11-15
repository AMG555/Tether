/**
 * companion-overlay.js
 * Runs inside the transparent companion-overlay window.
 *
 * Rendering priority:
 *   1. WebP sprite frames from installed companion pack (via IPC)
 *   2. Procedural canvas fallback (custom companions / no pack installed)
 *
 * Walk behaviour: companion auto-walks across the bottom of the screen
 * left → right then right → left. User can drag it to any position.
 *
 * All Tauri IPC calls must go through api.js (architecture rule: no direct Tauri globals).
 */

import { startCompanionDrag, getCompanionClip } from './api.js';

// ─── Screen walk constants ────────────────────────────────────────────────────

const WALK_PAUSE_MS = 6000;      // pause at each edge before turning
const WALK_SPEED_PX = 1.2;       // pixels per frame at 60fps
const IDLE_IDLE_CHANCE = 0.004;  // per-frame chance to pause mid-walk
const SPRITE_FPS = 12;           // WebP sprite playback rate

// ─── Companion data ─────────────────────────────────────────────────────────

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

// ─── Canvas setup ────────────────────────────────────────────────────────────

const canvas = document.getElementById('companion-canvas');
const ctx = canvas.getContext('2d');
const W = canvas.width;
const H = canvas.height;

// ─── State machine ───────────────────────────────────────────────────────────

const WALK_STATES = ['idle', 'walk', 'walk', 'wave', 'happy', 'water'];
let currentState = 'idle';
let nextStateTimer = 0;
let walkDir = 1;         // 1 = right, -1 = left
let walkX = W / 2;       // current x position of character centre
let walking = false;
let pauseTimer = 0;
let animTime = 0;
let rafId = null;

// ─── WebP sprite cache ───────────────────────────────────────────────────────

/** @type {Record<string, HTMLImageElement | null>} state → loaded Image or null */
const spriteCache = {};
/** @type {Record<string, boolean>} state → currently loading */
const spriteLoading = {};

async function loadSprite(companionId, state) {
  const key = `${companionId}:${state}`;
  if (key in spriteCache) return spriteCache[key];
  if (spriteLoading[key]) return undefined; // still loading
  spriteLoading[key] = true;
  try {
    const dataUrl = await getCompanionClip(companionId, state);
    if (dataUrl) {
      const img = new Image();
      img.src = dataUrl;
      await new Promise((res) => { img.onload = res; img.onerror = res; });
      spriteCache[key] = img;
      return img;
    }
  } catch { /* fall through */ }
  spriteCache[key] = null; // mark as unavailable so we don't retry
  return null;
}

// Pre-load idle sprite immediately
loadSprite(companion.id, 'idle');

// ─── State helpers ───────────────────────────────────────────────────────────

function pickNextState() {
  const idx = Math.floor(Math.random() * WALK_STATES.length);
  currentState = WALK_STATES[idx];
  walking = currentState === 'walk';
  nextStateTimer = performance.now() + (5000 + Math.random() * 6000);
  // Pre-load sprite for the new state
  loadSprite(companion.id, currentState);
}

pickNextState();

// ─── Render loop ─────────────────────────────────────────────────────────────

let lastTimestamp = 0;
let spriteFrame = 0;
let spriteFrameTimer = 0;

function renderFrame(timestamp) {
  rafId = null;
  const dt = Math.min(timestamp - lastTimestamp, 50); // cap at 50ms
  lastTimestamp = timestamp;
  animTime += dt * 0.001; // seconds

  // Advance sprite frame
  spriteFrameTimer += dt;
  if (spriteFrameTimer > 1000 / SPRITE_FPS) {
    spriteFrame++;
    spriteFrameTimer = 0;
  }

  // State transition
  if (timestamp > nextStateTimer) pickNextState();

  // Walk position update
  if (walking) {
    if (Math.random() < IDLE_IDLE_CHANCE) {
      walking = false;
      currentState = 'idle';
      nextStateTimer = performance.now() + 3000;
    } else {
      walkX += walkDir * WALK_SPEED_PX;
      // Bounce at edges (leave a 20px margin)
      if (walkX > W - 20) {
        walkX = W - 20;
        walkDir = -1;
        walking = false;
        pauseTimer = timestamp + WALK_PAUSE_MS;
        currentState = 'idle';
      } else if (walkX < 20) {
        walkX = 20;
        walkDir = 1;
        walking = false;
        pauseTimer = timestamp + WALK_PAUSE_MS;
        currentState = 'idle';
      }
    }
  } else if (pauseTimer > 0 && timestamp > pauseTimer) {
    // Resume walk after edge pause
    pauseTimer = 0;
    walking = true;
    currentState = 'walk';
    nextStateTimer = performance.now() + 8000;
  }

  // Draw
  ctx.clearRect(0, 0, W, H);
  drawCompanion();

  rafId = requestAnimationFrame(renderFrame);
}

function drawCompanion() {
  const spriteKey = `${companion.id}:${currentState}`;
  const img = spriteCache[spriteKey];

  // Flip horizontally when walking left
  const shouldFlip = walking && walkDir === -1;

  if (img) {
    // WebP sprite rendering
    const iw = img.naturalWidth;
    const ih = img.naturalHeight;
    const scale = Math.min(W / iw, H / ih, 1);
    const dw = iw * scale;
    const dh = ih * scale;
    const dx = walkX - dw / 2;
    const dy = H - dh;

    ctx.save();
    if (shouldFlip) {
      ctx.translate(walkX * 2, 0);
      ctx.scale(-1, 1);
    }
    ctx.drawImage(img, dx, dy, dw, dh);
    ctx.restore();
  } else {
    // Procedural fallback — also load sprite in background if not attempted
    if (!(spriteKey in spriteCache) && !spriteLoading[spriteKey]) {
      loadSprite(companion.id, currentState);
    }
    drawProceduralCompanion(shouldFlip);
  }
}

function drawProceduralCompanion(flip) {
  const colors = companion.colors || DEFAULT_COMPANION.colors;
  const cx = walkX;
  const baseY = H - 20; // stand on bottom of canvas

  let bob = Math.sin(animTime * 3) * 3;
  let waveAngle = 0;
  let walkPhase = 0;
  let eyeBlink = Math.sin(animTime * 0.8) > 0.95 ? 0.1 : 1.0;

  if (currentState === 'walk') {
    walkPhase = Math.sin(animTime * 6);
    bob = Math.abs(Math.sin(animTime * 6)) * 5;
  } else if (currentState === 'wave') {
    waveAngle = Math.sin(animTime * 8) * 0.5;
  } else if (currentState === 'happy') {
    bob = Math.abs(Math.sin(animTime * 8)) * 10;
    waveAngle = Math.sin(animTime * 12) * 0.4;
  }

  ctx.save();
  if (flip) {
    ctx.translate(cx * 2, 0);
    ctx.scale(-1, 1);
  }

  // Ground shadow
  ctx.fillStyle = 'rgba(0,0,0,0.18)';
  ctx.beginPath();
  ctx.ellipse(cx, baseY + 2, 22, 5, 0, 0, Math.PI * 2);
  ctx.fill();

  // Legs
  ctx.fillStyle = colors.accent || '#c2331a';
  const legL = walkPhase * 7;
  const legR = -walkPhase * 7;
  ctx.fillRect(cx - 14, baseY - 15 - bob + legL, 9, 14);
  ctx.fillRect(cx + 5,  baseY - 15 - bob + legR, 9, 14);

  // Body
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(cx - 20, baseY - 55 - bob, 40, 42, 6);
  ctx.fill();

  // Accent chest badge
  ctx.fillStyle = colors.accent || '#c2331a';
  ctx.beginPath();
  ctx.roundRect(cx - 9, baseY - 42 - bob, 18, 20, 3);
  ctx.fill();

  // Left arm
  ctx.save();
  ctx.translate(cx - 20, baseY - 46 - bob);
  ctx.rotate(currentState === 'wave' ? -1.1 + waveAngle : 0.15 - walkPhase * 0.25);
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.fillRect(-7, 0, 7, 22);
  ctx.restore();

  // Right arm
  ctx.save();
  ctx.translate(cx + 20, baseY - 46 - bob);
  ctx.rotate(currentState === 'wave' ? 1.1 - waveAngle : -0.15 + walkPhase * 0.25);
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.fillRect(0, 0, 7, 22);
  ctx.restore();

  // Head
  ctx.fillStyle = colors.body || '#2563eb';
  ctx.beginPath();
  ctx.roundRect(cx - 17, baseY - 86 - bob, 34, 28, 6);
  ctx.fill();

  // Ears
  ctx.fillStyle = colors.accent || '#c2331a';
  ctx.beginPath();
  ctx.moveTo(cx - 14, baseY - 86 - bob);
  ctx.lineTo(cx - 9,  baseY - 98 - bob);
  ctx.lineTo(cx - 4,  baseY - 86 - bob);
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(cx + 4,  baseY - 86 - bob);
  ctx.lineTo(cx + 9,  baseY - 98 - bob);
  ctx.lineTo(cx + 14, baseY - 86 - bob);
  ctx.fill();

  // Eyes
  ctx.fillStyle = colors.eyes || '#14120f';
  ctx.fillRect(cx - 11, baseY - 76 - bob, 5, 7 * eyeBlink);
  ctx.fillRect(cx + 6,  baseY - 76 - bob, 5, 7 * eyeBlink);

  // Action bubble
  if (currentState === 'water') {
    ctx.fillStyle = 'rgba(56,189,248,0.9)';
    ctx.beginPath();
    ctx.arc(cx + 28, baseY - 92 - bob, 8, 0, Math.PI * 2);
    ctx.fill();
    ctx.font = '10px sans-serif';
    ctx.fillText('💧', cx + 23, baseY - 88 - bob);
  }

  ctx.restore();
}

// ─── Animation control ───────────────────────────────────────────────────────

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

startAnim();

document.addEventListener('visibilitychange', () => {
  if (document.hidden) stopAnim(); else startAnim();
});

// ─── Drag to reposition ───────────────────────────────────────────────────────

canvas.addEventListener('mousedown', () => {
  startCompanionDrag();
});

// ─── Reload companion when activated from main window ────────────────────────

function reloadCompanion() {
  companion = loadActiveCompanion();
  // Clear sprite cache for the new companion so it re-fetches
  Object.keys(spriteCache).forEach((k) => {
    if (!k.startsWith(companion.id + ':')) delete spriteCache[k];
  });
  Object.keys(spriteLoading).forEach((k) => {
    if (!k.startsWith(companion.id + ':')) delete spriteLoading[k];
  });
  // Pre-load idle for new companion
  loadSprite(companion.id, 'idle');
  loadSprite(companion.id, 'walk');
  pickNextState();
}

window.addEventListener('companion-activated', reloadCompanion);

window.addEventListener('storage', (e) => {
  if (e.key === 'tether-active-companion') reloadCompanion();
});
