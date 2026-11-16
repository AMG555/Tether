/**
 * companion-brain.js
 *
 * Pure-logic session context model for the companion overlay.
 * Zero browser/Tauri dependencies — importable from Node tests.
 *
 * The brain observes the data the companion already has (keystroke counts,
 * draft counts, session time, time of day) and produces:
 *   • stateWeights()   — weighted multipliers that bias which animation state
 *                        is picked next (typing hard → more happy, long idle
 *                        → more water/stretch/eyes, etc.)
 *   • messageFor(state) — a context-sensitive speech-bubble line, or null to
 *                         fall back to the static DEFAULT_QUOTES table.
 *
 * The brain fires one-shot milestone celebrations (burst typing, long session,
 * drafts saved) via a callback supplied at construction time so the caller
 * can drive the companion animation without coupling the brain to DOM state.
 *
 * No network, no ML, no extra install footprint.  One optional localStorage
 * key persists the daily streak counter across sessions.
 */

// ─── Thresholds ───────────────────────────────────────────────────────────────

/** Events-per-minute burst threshold — "you're in the zone!" */
export const MILESTONE_BURST       = 200;
/** Session minutes before first long-session nudge */
export const MILESTONE_SESSION_MIN = 25;
/** Draft-save milestones (session-relative counts) */
export const MILESTONE_DRAFTS_1    = 1;
export const MILESTONE_DRAFTS_5    = 5;
export const MILESTONE_DRAFTS_10   = 10;
/** Minutes of zero new events before break-state weights escalate */
export const IDLE_WARN_MIN         = 8;
/** Sliding window for velocity calculation (ms) */
export const VELOCITY_WINDOW_MS    = 60_000;

// ─── Persistence helpers ──────────────────────────────────────────────────────

const BRAIN_STORAGE_KEY = 'tether_brain_session';

/**
 * Load persistent cross-session data from storage.
 * @param {Storage|null} storage  localStorage or a compatible shim; null = ephemeral.
 */
export function brainLoad(storage) {
  try {
    const raw = storage && storage.getItem(BRAIN_STORAGE_KEY);
    if (raw) return JSON.parse(raw);
  } catch { /* storage unavailable */ }
  return { totalDraftsEver: 0, lastSeenDate: '', streakDays: 0 };
}

/**
 * Persist cross-session data back to storage.
 * @param {Storage|null} storage
 * @param {object} data
 */
export function brainSave(storage, data) {
  try {
    if (storage) storage.setItem(BRAIN_STORAGE_KEY, JSON.stringify(data));
  } catch { /* ok */ }
}

// ─── SessionBrain factory ─────────────────────────────────────────────────────

/**
 * Create a new SessionBrain instance.
 *
 * @param {{
 *   storage?:   Storage|null,     — localStorage shim; null = no persistence
 *   onCelebrate?: (milestone: string, text: string) => void,
 *   nowMs?:     () => number       — injectable clock for tests (default Date.now)
 * }} opts
 */
export function createBrain({ storage = null, onCelebrate = null, nowMs = () => Date.now() } = {}) {
  // ── Persistent data ──
  const _persist = brainLoad(storage);

  // ── Session-local state ──
  let sessionStartMs       = nowMs();
  let eventsAtSessionStart = 0;
  let _startDrafts         = undefined;
  let sessionDrafts        = 0;
  let _lastActiveMs        = nowMs();
  let _lastEventsSeen      = 0;        // last known eventsSeen from the caller
  let _velReadings         = [];       // [{ ms, events }]
  const _firedMilestones   = new Set();

  // ── Velocity helper ──
  function _currentVelocity() {
    if (_velReadings.length < 2) return 0;
    const oldest = _velReadings[0];
    const newest = _velReadings[_velReadings.length - 1];
    const deltaEvents = newest.events - oldest.events;
    const deltaMin    = (newest.ms - oldest.ms) / 60_000;
    if (deltaMin < 0.01) return 0;
    return deltaEvents / deltaMin;
  }

  // ── Milestone celebration ──
  function _celebrate(milestone) {
    const msgs = {
      burst:     ['🔥 You\'re in the zone!', '⚡ Blazing speed!', '🚀 Look at you go!'],
      session25: ['⏱️ 25 min in — nice focus!', '🎯 Solid session so far!'],
      session60: ['🏆 1 hour straight — champion!', '💪 60-min session — take a break soon!'],
      draft1:    ['💾 First draft saved!', '✅ Draft #1 secured!'],
      draft5:    ['🌟 5 drafts saved this session!', '📚 Five in the bank!'],
      draft10:   ['🎊 10 drafts saved — incredible!', '🏅 Draft machine!'],
    };
    const pool = msgs[milestone] || [];
    if (!pool.length) return;
    const text = pool[Math.floor(Math.random() * pool.length)];
    if (onCelebrate) onCelebrate(milestone, text);
  }

  // ── Milestone checker ──
  function _checkMilestones(now) {
    const velocity       = _currentVelocity();
    const sessionMinutes = (now - sessionStartMs) / 60_000;

    if (velocity > MILESTONE_BURST / (VELOCITY_WINDOW_MS / 60_000)
        && !_firedMilestones.has('burst')) {
      _firedMilestones.add('burst');
      _celebrate('burst');
    }
    if (sessionMinutes > MILESTONE_SESSION_MIN && !_firedMilestones.has('session25')) {
      _firedMilestones.add('session25');
      _celebrate('session25');
    }
    if (sessionMinutes > 60 && !_firedMilestones.has('session60')) {
      _firedMilestones.add('session60');
      _celebrate('session60');
    }
    if (sessionDrafts >= MILESTONE_DRAFTS_1  && !_firedMilestones.has('draft1'))  { _firedMilestones.add('draft1');  _celebrate('draft1');  }
    if (sessionDrafts >= MILESTONE_DRAFTS_5  && !_firedMilestones.has('draft5'))  { _firedMilestones.add('draft5');  _celebrate('draft5');  }
    if (sessionDrafts >= MILESTONE_DRAFTS_10 && !_firedMilestones.has('draft10')) { _firedMilestones.add('draft10'); _celebrate('draft10'); }
  }

  // ── Returning-user streak ──
  function _checkReturningUser() {
    const today     = new Date(nowMs()).toISOString().slice(0, 10);
    if (_persist.lastSeenDate === today) return;

    const yesterday = new Date(nowMs() - 86_400_000).toISOString().slice(0, 10);
    _persist.streakDays = _persist.lastSeenDate === yesterday
      ? (_persist.streakDays || 1) + 1
      : 1;
    _persist.lastSeenDate = today;
    brainSave(storage, _persist);
    return _persist.streakDays;
  }

  // ── Public API object ──
  return {
    /** Feed the brain each time getCaptureStatus returns new data. */
    observe(eventsSeen, draftsSaved, capturing) {
      const now = nowMs();

      if (capturing && eventsAtSessionStart === 0 && eventsSeen > 0) {
        eventsAtSessionStart = eventsSeen;
        sessionStartMs = now;
        _checkReturningUser();
      }

      _velReadings.push({ ms: now, events: eventsSeen });
      _velReadings = _velReadings.filter(r => now - r.ms <= VELOCITY_WINDOW_MS);

      if (eventsSeen > _lastEventsSeen) _lastActiveMs = now;
      _lastEventsSeen = eventsSeen;

      if (_startDrafts === undefined && capturing) _startDrafts = draftsSaved;
      sessionDrafts = draftsSaved - (_startDrafts ?? draftsSaved);

      _checkMilestones(now);
    },

    /** Weighted multipliers for state picking. >1 = boost, <1 = suppress. */
    stateWeights() {
      const now            = nowMs();
      const sessionMinutes = (now - sessionStartMs) / 60_000;
      const idleMinutes    = (now - _lastActiveMs)  / 60_000;
      const hour           = new Date(now).getHours();
      const velocity       = _currentVelocity();
      const w              = {};

      if (velocity > 50) {
        w.happy = 2.5; w.water = 0.3; w.stretch = 0.3; w.eyes = 0.3;
      }
      if (idleMinutes > IDLE_WARN_MIN) {
        w.water   = (w.water   ?? 1) * 2.5;
        w.stretch = (w.stretch ?? 1) * 2.5;
        w.eyes    = (w.eyes    ?? 1) * 2.0;
        w.sad     = (w.sad     ?? 1) * 1.5;
        w.happy   = (w.happy   ?? 1) * 0.3;
      }
      if (sessionMinutes > MILESTONE_SESSION_MIN) {
        const factor = Math.min(3, 1 + (sessionMinutes - MILESTONE_SESSION_MIN) / 15);
        w.water   = (w.water   ?? 1) * factor;
        w.stretch = (w.stretch ?? 1) * factor;
        w.eyes    = (w.eyes    ?? 1) * factor;
      }
      if (hour >= 6  && hour < 11) { w.happy = (w.happy ?? 1) * 1.4; w.wave  = (w.wave  ?? 1) * 1.4; w.sad = (w.sad ?? 1) * 0.4; }
      if (hour >= 13 && hour < 15) { w.water = (w.water ?? 1) * 1.8; w.stretch = (w.stretch ?? 1) * 1.6; }
      if (hour >= 20)              { w.eyes  = (w.eyes  ?? 1) * 2.0; w.sad    = (w.sad    ?? 1) * 1.5; w.happy = (w.happy ?? 1) * 0.6; }

      return w;
    },

    /** Context-sensitive speech-bubble text, or null to use the static default. */
    messageFor(state) {
      const now            = nowMs();
      const hour           = new Date(now).getHours();
      const sessionMinutes = (now - sessionStartMs) / 60_000;
      const velocity       = _currentVelocity();
      const idleMinutes    = (now - _lastActiveMs) / 60_000;
      const drafts         = sessionDrafts;

      if (state === 'water') {
        if (sessionMinutes > 90) return '💧 You\'ve been at it 90+ min — water break!';
        if (hour >= 13 && hour < 15) return '💧 Post-lunch hydration check!';
        if (idleMinutes > IDLE_WARN_MIN) return '💧 Still here? Grab some water.';
        return null;
      }
      if (state === 'stretch') {
        if (sessionMinutes > 60) return '🧘 1 hour in — time for a 2-min stretch!';
        if (hour >= 20) return '🧘 Wind-down stretch — you earned it.';
        return null;
      }
      if (state === 'eyes') {
        if (hour >= 20) return '👁️ Late session — give your eyes a 20-sec rest.';
        if (sessionMinutes > 45) return '👁️ 20-20-20: look 20 ft away for 20 sec.';
        return null;
      }
      if (state === 'happy') {
        if (velocity > 80) return '🔥 You\'re on fire! Keep going!';
        if (drafts >= 5)   return `🎉 ${drafts} drafts saved this session!`;
        if (hour >= 6 && hour < 10) return '☀️ Morning momentum — nice start!';
        return null;
      }
      if (state === 'sad') {
        if (idleMinutes > IDLE_WARN_MIN * 1.5) return '😴 Still with us? No rush.';
        if (hour >= 22) return '🌙 Getting late — maybe call it a night?';
        return null;
      }
      if (state === 'wave') {
        if (_persist.streakDays > 1) return `👋 Day ${_persist.streakDays} streak — keep it up!`;
        return null;
      }
      return null;
    },

    /** Exposed for use in velocity-aware reactive messages in the overlay. */
    currentVelocity() { return _currentVelocity(); },

    /** Exposed for streak greeting in the overlay. */
    get streakDays() { return _persist.streakDays; },

    /** Exposed for milestone greetings used in the overlay's setTimeout callbacks. */
    get sessionDrafts() { return sessionDrafts; },
  };
}
