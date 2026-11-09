//! The `savepoint ask` companion: the desktop pet's command surface.
//!
//! Naming: `ask` because that is what the pet does. It asks you to drink water, and it asks
//! whether you want the thing you lost back. The subcommand list is deliberately verbs a
//! person would say out loud.
//!
//! This module holds no rendering. It owns:
//!   - `--new`      scaffold a valid, animated pet from procedural art
//!   - `--studio`   open the offline authoring page
//!   - `--list`     what's installed (and what's broken, and why)
//!   - `--install`  validate + copy a pet folder in
//!   - `--check`    simulate a reminder day, so you can tune the schedule without waiting
//!   - bare `ask`   a live foreground companion for terminals: ASCII pet + real reminders

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::pet;
use crate::remind::{line_for, Kind, ReminderEngine, Trigger};

pub enum Action {
    New {
        name: String,
        frame_size: u32,
        body: String,
        accent: String,
    },
    Studio,
    List,
    Install {
        path: PathBuf,
    },
    Check,
    Run {
        ascii: bool,
    },
}


fn pets_dir(data: &Path) -> PathBuf {
    data.join("pets")
}

pub fn run(data: &Path, action: Action) -> Result<(), Box<dyn std::error::Error>> {
    match action {

        Action::New { name, frame_size, body, accent } => {
            let slug = pet::slug(&name);
            let dir = pets_dir(data).join(&slug);
            if dir.exists() {
                return Err(format!(
                    "{} already exists — pick another name or remove it",
                    dir.display()
                )
                .into());
            }
            let p = pet::scaffold(&dir, &name, frame_size, &body, &accent)?;
            println!("created pet {:?} at {}", p.name, dir.display());
            println!(
                "  sprite.png  {}×{} px, 15 frames, 5 animations",
                p.frame_size * 4,
                p.frame_size * 5
            );
            println!("  pet.json    editable — change fps, speech lines, scale");
            println!("\nThis is a sprite companion: one sheet, five animations. For a realistic");
            println!("character with all ten states (idle, walk, water, and more), author clips:");
            println!("  savepoint ask --character-studio");
            println!();
            println!("see it:  savepoint ask --list");
            println!("use it:  set \"pet\": \"{}\" in config.json, then savepoint ask", slug);
            println!("edit it: open sprite.png in any editor, or run `savepoint ask --studio`");
            Ok(())
        }
        Action::Studio => {
            let path = write_studio(data)?;
            println!("Pet Studio (offline authoring page): {}", path.display());
            println!("opening it in your browser…");
            open_path(&path);
            Ok(())
        }
        Action::List => {
            let (pets, bad) = pet::installed(&pets_dir(data));
            if pets.is_empty() && bad.is_empty() {
                println!("no pets installed yet.");
                println!("make one:  savepoint ask --new my-pet");
                return Ok(());
            }
            for p in &pets {
                println!(
                    "{:<16} {:<24} {}px ×{}  {}",
                    pet::slug(&p.name),
                    format!("{:?}", p.name),
                    p.frame_size,
                    p.scale,
                    if p.author.is_empty() { String::new() } else { format!("by {}", p.author) }
                );
            }
            for (path, why) in &bad {
                println!(
                    "{:<16} BROKEN: {}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    why
                );
            }
            Ok(())
        }
        Action::Install { path } => {
            let dest = pet::install(&path, &pets_dir(data))?;
            println!("installed to {}", dest.display());
            println!(
                "activate with: set \"pet\": \"{}\" in config.json",
                pet::slug(&dest.file_name().unwrap_or_default().to_string_lossy())
            );
            Ok(())
        }
        Action::Check => check(data),
        Action::Run { ascii } => run_live(data, ascii),
    
}
}



fn write_studio(data: &Path) -> std::io::Result<PathBuf> {
    // The studio is compiled into the binary so it works with no network and no checkout.
    const STUDIO: &str = include_str!("../assets/pet-studio.html");
    std::fs::create_dir_all(data)?;
    let path = data.join("pet-studio.html");
    std::fs::write(&path, STUDIO)?;
    Ok(path)
}

fn open_path(path: &Path) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd).arg(path).spawn();
}

/// Simulates a working day so the schedule can be tuned without waiting a day.
fn check(data: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let (cfg, warn) = Config::load_or_default(data);
    if let Some(w) = warn {
        eprintln!("warning: {w}");
    }
    let s = &cfg.remind;
    println!("reminder schedule");
    println!("  enabled      {}", s.enabled);
    println!("  water        every {} min", s.water_every_min);
    println!("  break        every {} min", s.break_every_min);
    if s.merge_break_and_posture {
        println!("  posture      merged into break");
    } else {
        println!("  posture      every {} min", s.posture_every_min);
    }
    println!("  quiet hours  {:02}:00–{:02}:00", s.quiet_hours.0, s.quiet_hours.1);
    println!("  cooldown     {} min between reminders", s.cooldown_min);
    println!("  session cap  {} reminders", s.max_per_session);
    println!();
    println!("simulated day (09:00–18:00 at the keyboard, lunch 12:30–13:30 away):\n");

    let mut e = ReminderEngine::new(s.clone(), Instant::now());
    let mut t = Instant::now();
    let mut fired = 0;
    let mut seed = 0;
    for minute in 0..(9 * 60) {
        t += Duration::from_secs(60);
        let clock_min = 9 * 60 + minute;
        let hour = (clock_min / 60) as u8;
        let mm = (clock_min % 60) as u8;
        let at_desk = !(12 * 60 + 30..13 * 60 + 30).contains(&clock_min);
        if let Some(trigger) = e.tick(t, at_desk, hour, mm) {
            let (kind, prefix) = match trigger {
                Trigger::Kind(k) => (k, ""),
                Trigger::WelcomeBack(k) => (k, "welcome back: "),
            };
            println!(
                "  {:02}:{:02}  {}{}  — {}",
                hour,
                mm,
                prefix,
                kind.key(),
                line_for(kind, None, seed)
            );
            seed += 1;
            fired += 1;
        }
    }
    println!("\n  {fired} reminders across a {}-hour day.", 9);
    if fired == 0 {
        println!("  that is suspiciously quiet — check quiet_hours and the intervals.");
    } else if fired > 30 {
        println!("  that is a lot. Consider raising the intervals or the cooldown.");
    }
    Ok(())
}

/// A foreground companion for people who live in a terminal. Same engine, ASCII clothes.
fn run_live(data: &Path, ascii: bool) -> Result<(), Box<dyn std::error::Error>> {
    let (cfg, warn) = Config::load_or_default(data);
    if let Some(w) = warn {
        eprintln!("warning: {w}");
    }
    let pet = cfg.resolve_pet(&pets_dir(data));
    let pet_name = pet.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "savepoint".into());
    let speech = pet.as_ref().map(|p| p.speech.clone());

    println!("{} is watching the clock (not you).", pet_name);
    println!(
        "reminders: water {}m · break {}m · quiet {:02}:00–{:02}:00",
        cfg.remind.water_every_min,
        cfg.remind.break_every_min,
        cfg.remind.quiet_hours.0,
        cfg.remind.quiet_hours.1
    );
    println!("Ctrl-C to stop.\n");

    let mut engine = ReminderEngine::new(cfg.remind.clone(), Instant::now());
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let s = std::sync::Arc::clone(&stop);
        let _ = std::panic::catch_unwind(|| {});
        crate::cli::install_sigint_pub(s);
    }

    let mut frame = 0usize;
    let mut seed = 0usize;
    let mut speaking_until: Option<Instant> = None;
    let mut last_status = Instant::now();

    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(250));
        let now = Instant::now();
        let (hour, minute) = local_hour_minute();

        // Presence proxy for a terminal companion: stdin is a tty somebody is using. We do not
        // read keystrokes here — that would be capture, and this process is not the capturer.
        let any_input = true;

        if let Some(trigger) = engine.tick(now, any_input, hour, minute) {
            let (kind, prefix) = match trigger {
                Trigger::Kind(k) => (k, ""),
                Trigger::WelcomeBack(k) => (k, "welcome back — "),
            };
            let custom = speech.as_ref().and_then(|s| s.for_kind(kind.key())).map(|v| v.to_vec());
            let line = line_for(kind, custom.as_deref(), seed);
            seed += 1;
            println!();
            println!("  {}  {prefix}{line}", frame_art(&pet_name, frame, ascii));
            speaking_until = Some(now + Duration::from_secs(8));
        }

        if last_status.elapsed() > Duration::from_secs(30) {
            last_status = now;
            let w = engine.remaining(Kind::Water).as_secs() / 60;
            let b = engine.remaining(Kind::Break).as_secs() / 60;
            // \r keeps it to one line so the companion does not scroll your terminal away.
            print!("\r  {}  water in {}m · break in {}m   ", idle_art(frame, ascii), w, b);
            use std::io::Write;
            let _ = std::io::stdout().flush();
            frame += 1;
        }
        if let Some(t) = speaking_until {
            if now > t {
                speaking_until = None;
                println!();
            }
        }
    }
    println!("\nbye.");
    Ok(())
}

fn local_hour_minute() -> (u8, u8) {
    // Local time without a dependency: read the offset from the OS where it is cheap, else UTC.
    // Accuracy matters only for quiet hours, so a rough offset is acceptable and documented.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let local = secs as i64 + local_offset_hours() as i64 * 3600;
    let day = local.rem_euclid(86_400);
    ((day / 3600) as u8, ((day % 3600) / 60) as u8)
}

#[cfg(unix)]
fn local_offset_hours() -> i32 {
    // `date +%z` is the portable no-dependency route: it respects TZ and DST.
    if let Ok(out) = std::process::Command::new("date").arg("+%z").output() {
        let s = String::from_utf8_lossy(&out.stdout);
        let s = s.trim();
        if s.len() == 5 {
            let sign = if s.starts_with('-') { -1 } else { 1 };
            if let (Ok(h), Ok(m)) = (s[1..3].parse::<i32>(), s[3..5].parse::<i32>()) {
                return sign * (h + m / 60);
            }
        }
    }
    0
}

#[cfg(not(unix))]
fn local_offset_hours() -> i32 {
    // Windows: the GUI/companion reads real local time through the OS API; the terminal
    // companion uses UTC-based hours, and quiet hours are documented as approximate there.
    0
}

fn frame_art(name: &str, frame: usize, ascii: bool) -> String {
    if ascii {
        let eyes = if frame % 2 == 0 { "o o" } else { "- -" };
        format!("( {eyes} ) < {name}")
    } else {
        let f = ["(◕‿◕)", "(◕o◕)", "(◕‿◕)", "(◕ω◕)"][frame % 4];
        format!("{f}  ")
    }
}

fn idle_art(frame: usize, ascii: bool) -> &'static str {
    if ascii {
        return if frame % 2 == 0 { "(o o)" } else { "(o o)~" };
    }
    match frame % 4 {
        0 => "(◕‿◕)",
        1 => "(◕‿◕)~",
        2 => "(◕ω◕)",
        _ => "(◕‿◕)",
    }
}
