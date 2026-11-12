//! Break, water and posture reminders — deliberately **behaviour-blind**.
//!
//! This is the module that makes the desktop companion more than decoration. It is also the
//! one that could quietly turn a privacy tool into surveillance, so the design refuses to know
//! anything about what you type:
//!
//! - Timing comes from a **monotonic clock** (`Instant`), never from analysing keystrokes.
//! - "Are you at the keyboard?" is one bit — *did any input arrive since the last check* —
//!   and that bit is consumed, not accumulated. No counts, no rates, no patterns.
//! - When you are away, the timer **pauses**. You are not nagged about a break you already
//!   took, and the tool has not learned anything about how fast you type.
//!
//! The consequence worth stating: Tether can tell you `time for water`, and cannot tell
//! you `you typed 4,000 words today`. That is a feature, and `remind::tests` asserts it: the
//! engine's entire input surface is `(now, any_input_since_last_tick)`.
//!
//! Schedules are simple, because a nagging pet gets uninstalled: a focus window, a quiet
//! window, a cooldown so a snooze isn't punished with an immediate repeat, and a hard cap on
//! how many reminders may fire in one session.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// What the pet should say. Adding a variant is a breaking change for `speech` lookups, so the
/// set stays small on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Water,
    Break,
    Posture,
}

impl Kind {
    pub fn key(&self) -> &'static str {
        match self {
            Kind::Water => "water",
            Kind::Break => "break",
            Kind::Posture => "posture",
        }
    }

    pub fn default_interval_min(&self) -> u64 {
        match self {
            Kind::Water => 45,
            Kind::Break => 20,
            Kind::Posture => 50,
        }
    }

    pub fn builtin_lines(&self) -> &'static [&'static str] {
        match self {
            Kind::Water => &[
                "water break 💧",
                "drink something, you're 60% water",
                "hydrate — your headache says hi",
            ],
            Kind::Break => &[
                "look 20 feet away for 20 seconds",
                "stand up for a minute",
                "shoulders down, jaw unclenched",
            ],
            Kind::Posture => {
                &["sit back, feet flat", "screen at eye level", "unround those shoulders"]
            }
        }
    }
}

fn default_water() -> u64 {
    45
}
fn default_break() -> u64 {
    20
}
fn default_posture() -> u64 {
    50
}
fn default_quiet() -> (u8, u8) {
    (22, 7)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    /// Master switch. Reminders are on by default because a pet that never speaks is furniture.
    pub enabled: bool,
    #[serde(default = "default_water")]
    pub water_every_min: u64,
    #[serde(default = "default_break")]
    pub break_every_min: u64,
    #[serde(default = "default_posture")]
    pub posture_every_min: u64,
    /// Apply `break_every_min` to posture too, instead of tracking it separately.
    pub merge_break_and_posture: bool,
    /// Local-time quiet window `(start_hour, end_hour)`, 24h, wrapping past midnight.
    /// Reminders are suppressed and their timers pause. Default 22:00–07:00.
    #[serde(default = "default_quiet")]
    pub quiet_hours: (u8, u8),
    /// Minimum gap between any two reminders, whatever the schedule says.
    pub cooldown_min: u64,
    /// Pause after you return from being away: nobody wants a reminder the instant they sit down.
    pub grace_after_return_min: u64,
    /// Hard cap per session, so a broken clock or a long day cannot produce a nagging loop.
    pub max_per_session: u32,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            enabled: true,
            water_every_min: default_water(),
            break_every_min: default_break(),
            posture_every_min: default_posture(),
            merge_break_and_posture: false,
            quiet_hours: default_quiet(),
            cooldown_min: 5,
            grace_after_return_min: 2,
            max_per_session: 24,
        }
    }
}

impl Schedule {
    pub fn interval(&self, kind: Kind) -> Duration {
        let m = match kind {
            Kind::Water => self.water_every_min,
            Kind::Break => self.break_every_min,
            Kind::Posture => {
                if self.merge_break_and_posture {
                    self.break_every_min
                } else {
                    self.posture_every_min
                }
            }
        };
        // Floors: a 0-minute interval would fire continuously, and a 10-second one is
        // indistinguishable from harassment.
        Duration::from_secs(m.clamp(5, 8 * 60) * 60)
    }

    pub fn kinds(&self) -> Vec<Kind> {
        if self.merge_break_and_posture {
            vec![Kind::Water, Kind::Break]
        } else {
            vec![Kind::Water, Kind::Break, Kind::Posture]
        }
    }

    /// Is `hour` (0–23) inside the quiet window? Handles windows that wrap midnight.
    pub fn is_quiet_at(&self, hour: u8, minute: u8) -> bool {
        let (start, end) = self.quiet_hours;
        if start == end {
            return false; // an empty window means "no quiet hours"
        }
        let now = minute as u32 + (hour as u32) * 60;
        let s = (start as u32) * 60;
        let e = (end as u32) * 60;
        if s < e {
            now >= s && now < e
        } else {
            now >= s || now < e
        }
    }
}

/// Why a reminder fired. Useful for the CLI output and for tests that would otherwise be
/// asserting on floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Kind(Kind),
    /// Suppressed because you were away — and here's what you missed, once.
    WelcomeBack(Kind),
}

/// A monotonically timed scheduler with no knowledge of content.
pub struct ReminderEngine {
    schedule: Schedule,
    /// Per-kind: how long has been spent *present* since the last fire.
    accrued: Vec<(Kind, Duration)>,
    last_tick: Instant,
    last_any_reminder: Option<Instant>,
    /// Set while the user is away; accrual pauses and a welcome-back is offered on return.
    away: bool,
    /// A kind that matured while away, held until they're back.
    deferred: Option<Kind>,
    fired_this_session: u32,
    pub quiet_now: bool,
}

impl ReminderEngine {
    pub fn new(schedule: Schedule, now: Instant) -> Self {
        let kinds = schedule.kinds();
        Self {
            accrued: kinds.into_iter().map(|k| (k, Duration::ZERO)).collect(),
            schedule,
            last_tick: now,
            last_any_reminder: None,
            away: false,
            deferred: None,
            fired_this_session: 0,
            quiet_now: false,
        }
    }

    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    /// Present-time accrued for a kind — exposed for `status`, and so tests can assert the
    /// pause-on-idle behaviour without sleeping.
    pub fn accrued(&self, kind: Kind) -> Duration {
        self.accrued.iter().find(|(k, _)| *k == kind).map(|(_, d)| *d).unwrap_or_default()
    }

    pub fn remaining(&self, kind: Kind) -> Duration {
        self.schedule.interval(kind).saturating_sub(self.accrued(kind))
    }

    /// The whole input surface: the current time, whether any input arrived since the last
    /// tick, and the current wall-clock hour/minute (for quiet hours only — never used to
    /// profile behaviour).
    ///
    /// `any_input` is a **bool**, and that is the point: the caller cannot hand this module a
    /// keystroke, a count, or a timestamp of what was typed. There is nothing here to mine.
    pub fn tick(&mut self, now: Instant, any_input: bool, hour: u8, minute: u8) -> Option<Trigger> {
        let delta = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;

        if !self.schedule.enabled {
            return None;
        }

        let quiet = self.schedule.is_quiet_at(hour, minute);
        self.quiet_now = quiet;

        // Presence detection: away after a generously long silence, back on the first input.
        // "Idle" uses a fixed threshold rather than a learned one — no behavioural model.
        const AWAY_AFTER: Duration = Duration::from_secs(150);
        let was_away = self.away;
        if !any_input && delta > AWAY_AFTER {
            self.away = true;
        } else if any_input {
            self.away = false;
        }

        if was_away && !self.away {
            // Just came back: don't stack a reminder on top of a return, and give grace.
            if let Some(kind) = self.deferred.take() {
                self.last_any_reminder = Some(now);
                return Some(Trigger::WelcomeBack(kind));
            }
        }

        if self.away || quiet {
            // Time away and time asleep are not screen time. This is also why the pet never
            // nags you about a break you already took.
            return None;
        }

        for (_kind, acc) in self.accrued.iter_mut() {
            *acc = acc.saturating_add(delta);
        }

        if self.fired_this_session >= self.schedule.max_per_session {
            return None;
        }

        // Cooldown: one reminder at a time, and never two in quick succession.
        if let Some(last) = self.last_any_reminder {
            if now.saturating_duration_since(last)
                < Duration::from_secs(self.schedule.cooldown_min * 60)
            {
                return None;
            }
        }

        // Fire whichever is most overdue, not whichever comes first in the list.
        let mut best: Option<(Kind, f64)> = None;
        for (kind, acc) in self.accrued.iter() {
            let interval = self.schedule.interval(*kind);
            if *acc >= interval {
                let over = acc.as_secs_f64() / interval.as_secs_f64();
                if best.map(|(_, b)| over > b).unwrap_or(true) {
                    best = Some((*kind, over));
                }
            }
        }

        if let Some((kind, _)) = best {
            self.reset(kind);
            self.fired_this_session += 1;
            self.last_any_reminder = Some(now);
            return Some(Trigger::Kind(kind));
        }
        None
    }

    fn reset(&mut self, kind: Kind) {
        for (k, acc) in self.accrued.iter_mut() {
            if *k == kind {
                *acc = Duration::ZERO;
            }
        }
    }

    /// Snooze: reset the timer so the next reminder for this kind is a full interval away.
    pub fn snooze(&mut self, kind: Kind) {
        self.reset(kind);
    }

    /// "Leave me alone until tomorrow": stop firing for the rest of this session.
    pub fn quiet_for_session(&mut self) {
        self.fired_this_session = self.schedule.max_per_session;
    }
}

/// Renders a reminder as the pet's speech bubble, picking a line so the same one doesn't repeat.
pub fn line_for(kind: Kind, custom: Option<&[String]>, seed: usize) -> String {
    if let Some(lines) = custom {
        if !lines.is_empty() {
            return lines[seed % lines.len()].clone();
        }
    }
    let builtin = kind.builtin_lines();
    builtin[seed % builtin.len()].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60;

    fn eng(water: u64, brk: u64) -> ReminderEngine {
        let s = Schedule {
            water_every_min: water,
            break_every_min: brk,
            posture_every_min: 600,
            cooldown_min: 1,
            ..Default::default()
        };
        ReminderEngine::new(s, Instant::now())
    }

    fn eng_cooldown(water: u64, cooldown_min: u64) -> ReminderEngine {
        let s = Schedule {
            water_every_min: water,
            break_every_min: 600,
            posture_every_min: 600,
            cooldown_min,
            ..Default::default()
        };
        ReminderEngine::new(s, Instant::now())
    }

    #[test]
    fn fires_only_after_the_interval() {
        let mut e = eng(45, 600);
        let t0 = Instant::now();
        let mut t = t0;
        // 44 minutes of continuous presence: nothing.
        for _ in 0..44 {
            t += Duration::from_secs(MIN);
            assert_eq!(e.tick(t, true, 12, 0), None);
        }
        t += Duration::from_secs(MIN);
        assert_eq!(e.tick(t, true, 12, 0), Some(Trigger::Kind(Kind::Water)));
    }

    #[test]
    fn away_time_does_not_count_as_screen_time() {
        let mut e = eng(45, 600);
        let t0 = Instant::now();
        // 40 minutes present.
        let mut t = t0;
        for _ in 0..40 {
            t += Duration::from_secs(MIN);
            e.tick(t, true, 12, 0);
        }
        // Then 4 hours away (lunch). The pet must NOT ambush them on return.
        let return_at = t + Duration::from_secs(4 * 60 * MIN);
        assert_eq!(e.tick(return_at, false, 16, 0), None, "away tick accrues nothing");
        assert_eq!(e.tick(return_at, true, 16, 0), None, "no ambush on return");
        // 5 more present minutes finish the interval that was nearly done (40 + 5 >= 45).
        let t2 = return_at + Duration::from_secs(5 * MIN);
        assert_eq!(e.tick(t2, true, 16, 5), Some(Trigger::Kind(Kind::Water)));
    }

    #[test]
    fn quiet_hours_suppress_and_do_not_accrue() {
        let mut e = eng(5, 600);
        let t0 = Instant::now();
        let mut t = t0;
        // 02:00 is inside the default 22:00–07:00 window.
        for _ in 0..30 {
            t += Duration::from_secs(MIN);
            assert_eq!(e.tick(t, true, 2, 0), None, "no reminders at 2am");
        }
        assert!(e.quiet_now);
        assert!(
            e.accrued(Kind::Water) < Duration::from_secs(2 * MIN),
            "quiet time must not accrue"
        );
        // 08:00: the interval starts being counted from zero.
        t += Duration::from_secs(MIN);
        assert_eq!(e.tick(t, true, 8, 0), None);
        for _ in 0..5 {
            t += Duration::from_secs(MIN);
            let r = e.tick(t, true, 8, 0);
            if r.is_some() {
                assert_eq!(r, Some(Trigger::Kind(Kind::Water)));
                return;
            }
        }
        panic!("should have fired in the morning");
    }

    #[test]
    fn quiet_window_wraps_midnight() {
        let s = Schedule { quiet_hours: (22, 7), ..Default::default() };
        assert!(s.is_quiet_at(22, 0));
        assert!(s.is_quiet_at(23, 59));
        assert!(s.is_quiet_at(0, 0));
        assert!(s.is_quiet_at(6, 59));
        assert!(!s.is_quiet_at(7, 0));
        assert!(!s.is_quiet_at(12, 0));
        assert!(!s.is_quiet_at(21, 59));
    }

    #[test]
    fn empty_quiet_window_means_never_quiet() {
        let s = Schedule { quiet_hours: (9, 9), ..Default::default() };
        assert!(!s.is_quiet_at(9, 0));
    }

    #[test]
    fn cooldown_prevents_back_to_back_nagging() {
        // Interval 2 min, cooldown 5 min: the cooldown must dominate. The invariant that
        // matters is not a total count but the gap between any two reminders.
        let mut e = eng_cooldown(2, 5);
        let base = Instant::now();
        let mut fired_at = Vec::new();
        for i in 0..60u64 {
            let t = base + Duration::from_secs((i + 1) * MIN);
            if e.tick(t, true, 12, 0).is_some() {
                fired_at.push(i + 1);
            }
        }
        assert!(fired_at.len() >= 2, "should still remind: {fired_at:?}");
        for pair in fired_at.windows(2) {
            assert!(
                pair[1] - pair[0] >= 5,
                "reminders only {} min apart, cooldown is 5: {fired_at:?}",
                pair[1] - pair[0]
            );
        }
    }

    #[test]
    fn cooldown_zero_allows_every_interval() {
        let mut e = eng_cooldown(5, 0);
        let mut fired = 0;
        let base = std::time::Instant::now();
        for i in 0..60u64 {
            let t = base + Duration::from_secs((i + 1) * MIN);
            if e.tick(t, true, 12, 0).is_some() {
                fired += 1;
            }
        }
        assert!(fired >= 10, "a zero cooldown should allow roughly one per interval, got {fired}");
    }

    #[test]
    fn most_overdue_wins() {
        let s = Schedule {
            water_every_min: 5,
            break_every_min: 10,
            posture_every_min: 600,
            cooldown_min: 0,
            ..Default::default()
        };
        let mut e = ReminderEngine::new(s, Instant::now());
        let mut t = Instant::now();
        // Let both go overdue without ticking often, so water is 2 intervals overdue.
        for _ in 0..10 {
            t += Duration::from_secs(MIN);
            let _ = e.tick(t, true, 12, 0);
        }
        t += Duration::from_secs(12 * MIN);
        let r = e.tick(t, true, 12, 0);
        assert_eq!(r, Some(Trigger::Kind(Kind::Water)), "the most overdue fires first");
    }

    #[test]
    fn session_cap_is_absolute() {
        let s = Schedule {
            water_every_min: 5,
            break_every_min: 600,
            posture_every_min: 600,
            cooldown_min: 0,
            max_per_session: 3,
            ..Default::default()
        };
        let mut e = ReminderEngine::new(s, Instant::now());
        let mut t = Instant::now();
        let mut fired = 0;
        for _ in 0..200 {
            t += Duration::from_secs(MIN);
            if e.tick(t, true, 12, 0).is_some() {
                fired += 1;
            }
        }
        assert_eq!(fired, 3, "cap must hold");
    }

    #[test]
    fn snooze_defers_a_full_interval() {
        let mut e = eng(5, 600);
        let mut t = Instant::now();
        for _ in 0..5 {
            t += Duration::from_secs(MIN);
            let _ = e.tick(t, true, 12, 0);
        }
        e.snooze(Kind::Water);
        assert_eq!(e.remaining(Kind::Water), Duration::from_secs(5 * MIN));
    }

    #[test]
    fn welcome_back_is_offered_once_after_a_deferral() {
        let mut e = eng(5, 600);
        let t0 = Instant::now();
        let mut t = t0;
        for _ in 0..5 {
            t += Duration::from_secs(MIN);
            e.tick(t, true, 12, 0); // fires at the end
        }
        // Force a deferred state: mature while away, then return.
        e.deferred = Some(Kind::Water);
        e.away = true;
        let r = e.tick(t + Duration::from_secs(200), true, 12, 10);
        assert_eq!(r, Some(Trigger::WelcomeBack(Kind::Water)));
        // And it does not repeat.
        let r2 = e.tick(t + Duration::from_secs(210), true, 12, 11);
        assert_ne!(r2, Some(Trigger::WelcomeBack(Kind::Water)));
    }

    #[test]
    fn disabled_never_fires() {
        let s =
            Schedule { enabled: false, water_every_min: 5, cooldown_min: 0, ..Default::default() };
        let mut e = ReminderEngine::new(s, Instant::now());
        let mut t = Instant::now();
        for _ in 0..100 {
            t += Duration::from_secs(MIN);
            assert_eq!(e.tick(t, true, 12, 0), None);
        }
    }

    #[test]
    fn intervals_are_floored_to_something_sane() {
        let s = Schedule { water_every_min: 0, ..Default::default() };
        assert!(s.interval(Kind::Water) >= Duration::from_secs(5 * MIN));
        let s2 = Schedule { water_every_min: 100_000, ..Default::default() };
        assert!(s2.interval(Kind::Water) <= Duration::from_secs(8 * 60 * MIN));
    }

    #[test]
    fn custom_lines_are_used_and_rotate() {
        let custom = vec!["glug".to_string(), "sip".to_string()];
        assert_eq!(line_for(Kind::Water, Some(&custom), 0), "glug");
        assert_eq!(line_for(Kind::Water, Some(&custom), 1), "sip");
        assert_eq!(line_for(Kind::Water, Some(&custom), 2), "glug");
        assert!(!line_for(Kind::Break, None, 0).is_empty(), "builtin fallback");
    }

    /// The behavioural-integrity test: the engine's only input is a bool. This asserts the
    /// same presence pattern produces the same reminders regardless of *what* was typed,
    /// because there is no channel through which content could reach it.
    #[test]
    fn engine_cannot_see_content_by_construction() {
        let mut a = eng(10, 600);
        let mut b = eng(10, 600);
        let mut t = Instant::now();
        let mut out_a = vec![];
        let mut out_b = vec![];
        for i in 0..20 {
            t += Duration::from_secs(MIN);
            // Both get "input happened"; one is typing code, one is typing prose. The engine
            // signature cannot express the difference.
            out_a.push(a.tick(t, i % 7 != 0, 12, 0));
            out_b.push(b.tick(t, i % 7 != 0, 12, 0));
        }
        assert_eq!(out_a, out_b);
    }
}
