/**
 * companion-overlay.js
 * Runs inside the transparent companion-overlay window.
 * Renders the active companion procedurally, animates it, and
 * lets the user drag the window using Tauri's window API.
 */

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
      if (parsed && parsed.id && parsed.colors) return parsed;
    }
  } catch {
    // ignore parse errors
  }
  return DEFAULT_COMPANION;
}

// ─── State machine ───────────────────────────────────────────────────────────

const STATES = ['idle', 'idle', 'idle', 'walk', 'walk', 'wave', 'happy', 'water'];
// weights: idle=3/8, walk=2/8, wave=1/8, happy=1/8, water=1/8

let currentState = 'idle';
let nextStateTimer = 0;
let companion = loadActiveCompanion();

function pickNextState() {
  const idx = Math.floor(Math.random() * STATES.length);
  currentState = STATES[idx];
  // Stay in this state 4–8 seconds
  nextStateTimer = performance.now() + (4000 + Math.random() * 4000);
}

pickNextState();

// ─── Canvas drawing ──────────────────────────────────────────────────────────

const canvas = document.getElementById('companion-canvas');
const ctx = canvas.getContext('2d');
let animTime = 0;
let rafId = null;

function startAnim() {
  if (rafId !== null) return;
  rafId = requestAnimationFrame(renderFrame);
}

function stopAnim() {
  if (rafId !== null) {
    cancelAnimationFrame(rafId);
    rafId = null;
  }
}

function renderFrame(timestamp) {
  rafId = null;
  animTime += 0.04;

  // Switch state if timer elapsed
  if (timestamp > nextStateTimer) {
    pickNextState();
  }

  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);

  const cx = w / 2;
  const cy = h / 2 + 30;
  const colors = companion.colors || DEFAULT_COMPANION.colors;

  // Kinematics
  let bob = Math.sin(animTime * 3) * 4;
  let waveAngle = 0;
  let walkPhase = 0;
  let eyeBlink = Math.sin(animTime * 0.8) > 0.95 ? 0.1 : 1.0;

  if (currentState === 'walk') {
    walkPhase = Math.sin(animTime * 6);
    bob = Math.abs(Math.sin(animTime * 6)) * 6;
  } else if (currentState === 'wave') {
    waveAngle = Math.sin(animTime * 8) * 0.5;
  } else if (currentState === 'happy') {
    bob = Math.abs(Math.sin(animTime * 8)) * 14;
    waveAngle = Math.sin(animTime * 12) * 0.4;
  } else if (currentState === 'sad') {
    bob = -8;
    eyeBlink = 0.3;
  }

  // Ground shadow
  ctx.fillStyle = 'rgba(0,0,0,0.30)';
  ctx.beginPath();
  ctx.ellipse(cx, cy + 46, 32 - bob * 0.3, 9, 0, 0, Math.PI * 2);
  ctx.fill();

  // Body
  ctx.fillStyle = colors.body;
  ctx.beginPath();
  ctx.roundRect(cx - 24, cy - 30 - bob, 48, 54, 7);
  ctx.fill();

  // Accent chest badge
  ctx.fillStyle = colors.accent;
  ctx.beginPath();
  ctx.roundRect(cx - 11, cy - 14 - bob, 22, 24, 4);
  ctx.fill();

  // Legs
  ctx.fillStyle = colors.accent;
  const legL = walkPhase * 10;
  const legR = -walkPhase * 10;
  ctx.fillRect(cx - 18, cy + 24 - bob + legL, 11, 17);
  ctx.fillRect(cx + 7, cy + 24 - bob + legR, 11, 17);

  // Left arm
  ctx.save();
  ctx.translate(cx - 24, cy - 16 - bob);
  ctx.rotate(currentState === 'wave' ? -1.2 + waveAngle : 0.2 - walkPhase * 0.3);
  ctx.fillStyle = colors.body;
  ctx.fillRect(-8, 0, 8, 28);
  ctx.restore();

  // Right arm
  ctx.save();
  ctx.translate(cx + 24, cy - 16 - bob);
  ctx.rotate(currentState === 'wave' ? 1.2 - waveAngle : -0.2 + walkPhase * 0.3);
  ctx.fillStyle = colors.body;
  ctx.fillRect(0, 0, 8, 28);
  ctx.restore();

  // Head
  ctx.fillStyle = colors.body;
  ctx.beginPath();
  ctx.roundRect(cx - 21, cy - 68 - bob, 42, 34, 7);
  ctx.fill();

  // Ears
  ctx.fillStyle = colors.accent;
  ctx.beginPath();
  ctx.moveTo(cx - 18, cy - 68 - bob);
  ctx.lineTo(cx - 12, cy - 84 - bob);
  ctx.lineTo(cx - 6, cy - 68 - bob);
  ctx.fill();

  ctx.beginPath();
  ctx.moveTo(cx + 6, cy - 68 - bob);
  ctx.lineTo(cx + 12, cy - 84 - bob);
  ctx.lineTo(cx + 18, cy - 68 - bob);
  ctx.fill();

  // Eyes
  ctx.fillStyle = colors.eyes;
  ctx.fillRect(cx - 14, cy - 56 - bob, 6, 8 * eyeBlink);
  ctx.fillRect(cx + 8, cy - 56 - bob, 6, 8 * eyeBlink);

  // Action bubble
  if (currentState === 'water') {
    ctx.fillStyle = '#38bdf8';
    ctx.beginPath();
    ctx.arc(cx + 36, cy - 72 - bob, 9, 0, Math.PI * 2);
    ctx.fill();
    ctx.font = '11px sans-serif';
    ctx.fillText('💧', cx + 30, cy - 68 - bob);
  } else if (currentState === 'stretch') {
    ctx.fillStyle = '#f59e0b';
    ctx.beginPath();
    ctx.arc(cx + 36, cy - 72 - bob, 9, 0, Math.PI * 2);
    ctx.fill();
    ctx.font = '11px sans-serif';
    ctx.fillText('⚡', cx + 30, cy - 68 - bob);
  }

  rafId = requestAnimationFrame(renderFrame);
}

startAnim();

// Pause when window is hidden (user switched focus away from overlay)
document.addEventListener('visibilitychange', () => {
  if (document.hidden) { stopAnim(); } else { startAnim(); }
});

// ─── Drag to move window ─────────────────────────────────────────────────────

let dragStart = null;
let winPosAtDragStart = null;

canvas.addEventListener('mousedown', async (e) => {
  if (!window.__TAURI__) return;
  dragStart = { x: e.screenX, y: e.screenY };
  try {
    const pos = await window.__TAURI__.window.getCurrent().outerPosition();
    winPosAtDragStart = { x: pos.x, y: pos.y };
  } catch {
    winPosAtDragStart = { x: 0, y: 0 };
  }
});

window.addEventListener('mousemove', async (e) => {
  if (!dragStart || !winPosAtDragStart || !window.__TAURI__) return;
  const dx = e.screenX - dragStart.x;
  const dy = e.screenY - dragStart.y;
  try {
    await window.__TAURI__.window.getCurrent().setPosition(
      new window.__TAURI__.window.PhysicalPosition(
        winPosAtDragStart.x + dx,
        winPosAtDragStart.y + dy,
      ),
    );
  } catch {
    // ignore
  }
});

window.addEventListener('mouseup', () => {
  dragStart = null;
  winPosAtDragStart = null;
});

// ─── Reload companion when activated from main window ───────────────────────

window.addEventListener('companion-activated', () => {
  companion = loadActiveCompanion();
  pickNextState();
});

// Also listen via storage events (main window writes to localStorage)
window.addEventListener('storage', (e) => {
  if (e.key === 'tether-active-companion') {
    companion = loadActiveCompanion();
    pickNextState();
  }
});
