/**
 * companion-brain.test.js
 *
 * Unit tests for the SessionBrain pure-logic module.  No DOM, no Tauri.
 */

import assert from 'node:assert/strict';
import test   from 'node:test';

import {
  createBrain,
  brainLoad,
  brainSave,
  IDLE_WARN_MIN,
  MILESTONE_SESSION_MIN,
  MILESTONE_DRAFTS_1,
  MILESTONE_DRAFTS_5,
  VELOCITY_WINDOW_MS,
} from '../src/companion-brain.js';

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Minimal in-memory localStorage shim. */
function makeStorage() {
  const store = {};
  return {
    getItem:    (k)    => store[k] ?? null,
    setItem:    (k, v) => { store[k] = String(v); },
    removeItem: (k)    => { delete store[k]; },
  };
}

/** Create a brain whose clock starts at `startMs` and can be advanced. */
function makeBrain(startMs = 0, storage = null, onCelebrate = null) {
  let now = startMs;
  const brain = createBrain({
    storage,
    onCelebrate,
    nowMs: () => now,
  });
  return { brain, advance: (ms) => { now += ms; } };
}

// ─── brainLoad / brainSave ────────────────────────────────────────────────────

test('brainLoad returns default object when storage is empty', () => {
  const data = brainLoad(makeStorage());
  assert.deepEqual(data, { totalDraftsEver: 0, lastSeenDate: '', streakDays: 0 });
});

test('brainLoad returns default object when storage is null', () => {
  const data = brainLoad(null);
  assert.deepEqual(data, { totalDraftsEver: 0, lastSeenDate: '', streakDays: 0 });
});

test('brainSave and brainLoad round-trip data', () => {
  const storage = makeStorage();
  brainSave(storage, { totalDraftsEver: 7, lastSeenDate: '2026-01-01', streakDays: 3 });
  assert.deepEqual(brainLoad(storage), { totalDraftsEver: 7, lastSeenDate: '2026-01-01', streakDays: 3 });
});

test('brainSave does not throw when storage is null', () => {
  assert.doesNotThrow(() => brainSave(null, { streakDays: 1 }));
});

// ─── stateWeights — velocity ──────────────────────────────────────────────────

test('stateWeights boosts happy and suppresses breaks when typing fast', () => {
  const { brain, advance } = makeBrain(0);

  // Inject two velocity readings 30 s apart — 60 events/min > 50 threshold
  brain.observe(0, 0, true);
  advance(30_000);
  brain.observe(30, 0, true);  // 30 events in 30 s = 60/min

  const w = brain.stateWeights();
  assert.ok((w.happy ?? 1) > 1, 'happy should be boosted when typing fast');
  assert.ok((w.water ?? 1) < 1, 'water should be suppressed when typing fast');
  assert.ok((w.stretch ?? 1) < 1, 'stretch should be suppressed when typing fast');
});

test('stateWeights suppresses happy and boosts break states when idle too long', () => {
  const { brain, advance } = makeBrain(0);
  brain.observe(10, 0, true);
  // Advance past the idle-warn threshold without feeding new events
  advance((IDLE_WARN_MIN + 1) * 60_000);

  const w = brain.stateWeights();
  assert.ok((w.water   ?? 1) > 1, 'water should be boosted after long idle');
  assert.ok((w.stretch ?? 1) > 1, 'stretch should be boosted after long idle');
  assert.ok((w.happy   ?? 1) < 1, 'happy should be suppressed after long idle');
});

test('stateWeights escalates break states after a long session', () => {
  const { brain, advance } = makeBrain(0);
  brain.observe(10, 0, true);
  advance((MILESTONE_SESSION_MIN + 10) * 60_000);
  // Feed one more tick so the brain sees we are still active
  brain.observe(11, 0, true);

  const w = brain.stateWeights();
  assert.ok((w.water ?? 1) > 1, 'water should be escalated after a long session');
});

// ─── stateWeights — no negative-slot crash ────────────────────────────────────

test('stateWeights never produces weights that would cause NaN slot counts', () => {
  const { brain } = makeBrain(0);
  const w = brain.stateWeights();
  for (const [state, weight] of Object.entries(w)) {
    assert.ok(Number.isFinite(weight), `weight for ${state} must be a finite number`);
    assert.ok(weight >= 0, `weight for ${state} must be non-negative`);
  }
});

// ─── messageFor ───────────────────────────────────────────────────────────────

test('messageFor returns null for idle state (no message defined)', () => {
  const { brain } = makeBrain(0);
  assert.equal(brain.messageFor('idle'), null);
});

test('messageFor returns a string for water after a 90+ min session', () => {
  // Simulate 91 minutes elapsed since capture start
  const start = 0;
  const { brain, advance } = makeBrain(start);
  brain.observe(5, 0, true);
  advance(91 * 60_000);

  const msg = brain.messageFor('water');
  assert.ok(typeof msg === 'string' && msg.length > 0,
    'should return a contextual water message after 90 min');
  assert.ok(msg.includes('90+'), 'message should mention the 90-min threshold');
});

test('messageFor returns a string for stretch after 60+ min session', () => {
  const { brain, advance } = makeBrain(0);
  brain.observe(5, 0, true);
  advance(61 * 60_000);

  const msg = brain.messageFor('stretch');
  assert.ok(typeof msg === 'string', 'should return a string for stretch after 60 min');
  assert.ok(msg.includes('1 hour'), 'stretch message should mention 1 hour');
});

test('messageFor returns draft count for happy when session drafts >= 5', () => {
  // Use a fixed start time well outside morning hours so the morning branch
  // doesn't fire, and advance far enough that velocity stays low (≤80/min).
  const startMs = new Date('2026-06-01T16:00:00Z').getTime(); // 4 PM UTC
  const { brain, advance } = makeBrain(startMs);
  brain.observe(5, 0, true);
  // Advance 2 minutes with no new events so velocity drops to ≤80/min
  advance(2 * 60_000);
  brain.observe(5, 5, true);  // same event count — no velocity burst; 5 drafts saved

  const msg = brain.messageFor('happy');
  assert.ok(typeof msg === 'string', 'should return a string for happy with 5+ drafts');
  assert.ok(msg.includes('5'), 'happy message should mention the draft count');
});

test('messageFor falls back to null when no context condition is met', () => {
  // Fresh brain, no observations — sessionMinutes ≈ 0, velocity = 0
  const { brain } = makeBrain(0);
  assert.equal(brain.messageFor('water'),   null, 'no context → no water message');
  assert.equal(brain.messageFor('stretch'), null, 'no context → no stretch message');
  assert.equal(brain.messageFor('eyes'),    null, 'no context → no eyes message');
  assert.equal(brain.messageFor('happy'),   null, 'no context → no happy message');
});

// ─── Milestone celebrations ───────────────────────────────────────────────────

test('draft1 milestone fires exactly once when first draft is saved', () => {
  const fired = [];
  const { brain, advance } = makeBrain(0, null, (m) => fired.push(m));

  brain.observe(5, 0, true);    // baseline: 0 drafts
  advance(1000);
  brain.observe(10, 1, true);   // 1st draft saved → milestone should fire

  assert.equal(fired.filter(m => m === 'draft1').length, 1,
    'draft1 milestone should fire exactly once');
});

test('draft1 milestone does not fire a second time', () => {
  const fired = [];
  const { brain, advance } = makeBrain(0, null, (m) => fired.push(m));

  brain.observe(5, 0, true);
  advance(1000);
  brain.observe(10, 1, true);
  advance(1000);
  brain.observe(15, 1, true);   // still 1 draft — should not fire again

  assert.equal(fired.filter(m => m === 'draft1').length, 1,
    'draft1 milestone fires at most once per session');
});

test('draft5 milestone fires when session drafts reach 5', () => {
  const fired = [];
  const { brain, advance } = makeBrain(0, null, (m) => fired.push(m));

  brain.observe(5, 0, true);
  advance(1000);
  brain.observe(10, 5, true);  // jump straight to 5 drafts

  assert.ok(fired.includes('draft5'), 'draft5 milestone should fire at 5 session drafts');
});

test('session25 milestone fires after MILESTONE_SESSION_MIN minutes', () => {
  const fired = [];
  const { brain, advance } = makeBrain(0, null, (m) => fired.push(m));

  brain.observe(5, 0, true);
  advance((MILESTONE_SESSION_MIN + 1) * 60_000);
  brain.observe(6, 0, true);  // poll tick to trigger milestone check

  assert.ok(fired.includes('session25'),
    'session25 milestone should fire after ' + MILESTONE_SESSION_MIN + ' min');
});

test('session25 milestone fires only once per session', () => {
  const fired = [];
  const { brain, advance } = makeBrain(0, null, (m) => fired.push(m));

  brain.observe(5, 0, true);
  advance((MILESTONE_SESSION_MIN + 1) * 60_000);
  brain.observe(6, 0, true);
  advance(5 * 60_000);
  brain.observe(7, 0, true);

  assert.equal(fired.filter(m => m === 'session25').length, 1,
    'session25 fires at most once per session');
});

// ─── Velocity calculation ─────────────────────────────────────────────────────

test('currentVelocity returns 0 with fewer than 2 readings', () => {
  const { brain } = makeBrain(0);
  assert.equal(brain.currentVelocity(), 0);
});

test('currentVelocity computes events-per-minute correctly', () => {
  const { brain, advance } = makeBrain(0);
  brain.observe(0, 0, true);
  advance(60_000);             // 1 minute
  brain.observe(120, 0, true); // 120 events in 1 min

  const v = brain.currentVelocity();
  assert.ok(Math.abs(v - 120) < 1, `expected ~120 events/min, got ${v}`);
});

test('currentVelocity drops readings older than VELOCITY_WINDOW_MS', () => {
  const { brain, advance } = makeBrain(0);
  brain.observe(0, 0, true);
  advance(VELOCITY_WINDOW_MS + 1000); // push first reading outside the window
  brain.observe(1, 0, true);           // only 1 reading inside the window

  // With only 1 reading inside the window, velocity should be 0
  assert.equal(brain.currentVelocity(), 0,
    'old readings outside the velocity window should be dropped');
});

// ─── Streak / persistent state ────────────────────────────────────────────────

test('streakDays starts at 0 for a brand-new storage', () => {
  const { brain } = makeBrain(0, makeStorage());
  assert.equal(brain.streakDays, 0);
});

test('streak increments to 1 on first capture and persists to storage', () => {
  const storage = makeStorage();
  const startMs = new Date('2026-06-01T09:00:00Z').getTime();
  const { brain } = makeBrain(startMs, storage);

  brain.observe(5, 0, true);  // first capture on 2026-06-01

  assert.ok(brain.streakDays >= 1, 'streakDays should be at least 1 after first capture');

  // Reload a fresh brain from the same storage — streak should be persisted
  const brain2 = createBrain({ storage, nowMs: () => startMs });
  assert.equal(brain2.streakDays, brain.streakDays, 'streak should survive a reload');
});

test('streak increments on consecutive days', () => {
  const storage = makeStorage();

  const day1Ms = new Date('2026-06-01T09:00:00Z').getTime();
  const day2Ms = new Date('2026-06-02T09:00:00Z').getTime();

  // Day 1
  const b1 = createBrain({ storage, nowMs: () => day1Ms });
  b1.observe(5, 0, true);

  // Day 2
  const b2 = createBrain({ storage, nowMs: () => day2Ms });
  b2.observe(5, 0, true);

  assert.equal(b2.streakDays, 2, 'streak should be 2 on the second consecutive day');
});

test('streak resets to 1 after a gap day', () => {
  const storage = makeStorage();

  const day1Ms = new Date('2026-06-01T09:00:00Z').getTime();
  const day3Ms = new Date('2026-06-03T09:00:00Z').getTime(); // skipped day 2

  const b1 = createBrain({ storage, nowMs: () => day1Ms });
  b1.observe(5, 0, true);

  const b3 = createBrain({ storage, nowMs: () => day3Ms });
  b3.observe(5, 0, true);

  assert.equal(b3.streakDays, 1, 'streak should reset to 1 after a gap');
});
