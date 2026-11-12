//! The `tether ask` companion: the desktop pet's command surface.
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
    /// Audit an avatar file (`.vrm`/`.glb`/`.gltf`) without loading it.
    Audit {
        path: PathBuf,
    },
    /// List avatars in the data directory, with their audit summary.
    Avatars,
    /// Open the offline Character Studio (clip-pack authoring) page.
    CharacterStudio,
    /// Add or replace one clip on an installed pet.
    SetClip {
        pet: Option<String>,
        state: String,
        file: PathBuf,
    },
    /// Remove one optional clip.
    UnsetClip {
        pet: Option<String>,
        state: String,
    },
    /// Print the animation states a pet has.
    Clips {
        pet: Option<String>,
    },
    Run {
        ascii: bool,
    },
}

/// Prints a full audit for a single avatar file.
///
/// Every line is something the user can act on: a blocked file says what to remove, a warning
/// names the exact metadata string, and an informational line explains why it is not a risk.
fn audit_avatar(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use crate::avatar::audit::{audit, Severity};
    let a = audit(path)?;
    let fmt = a.format.map(|f| f.label()).unwrap_or("unrecognised");
    println!("{}  ({fmt})", path.display());
    println!("  {}", a.summary());
    if a.humanoid_bones > 0 {
        println!("  humanoid rig: {} bones — the companion can animate this", a.humanoid_bones);
    }
    for img in &a.images {
        println!(
            "  image: {}×{} {} bytes{}",
            img.width,
            img.height,
            img.bytes,
            if img.photo_like { " — looks photographic" } else { "" }
        );
    }
    if a.findings.is_empty() {
        println!("\nno findings. this avatar is safe to load.");
        return Ok(());
    }
    println!();
    for f in &a.findings {
        let tag = match f.severity {
            Severity::Block => "BLOCK",
            Severity::Warn => "warn ",
            Severity::Info => "info ",
        };
        println!("  [{tag}] {}", f.what);
    }
    match a.worst() {
        Severity::Block => {
            println!(
                "\nrefused: {} blocking finding(s). remove them and re-run.",
                a.findings.iter().filter(|f| f.severity == Severity::Block).count()
            );
            Err("avatar has blocking findings".into())
        }
        _ => {
            println!("\nno blocking findings — review the warnings above, then load it.");
            Ok(())
        }
    }
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
            println!("  tether ask --character-studio");
            println!();
            println!("see it:  tether ask --list");
            println!("use it:  set \"pet\": \"{}\" in config.json, then tether ask", slug);
            println!("edit it: open sprite.png in any editor, or run `tether ask --studio`");
            Ok(())
        }
        Action::Audit { path } => {
            audit_avatar(&path)?;
            Ok(())
        }
        Action::SetClip { pet, state, file } => {
            let dir = crate::pack::resolve_pet(data, pet.as_deref())?;
            let changed = crate::pack::add_clip(&dir, &state, &file, 0)?;
            println!(
                "{} {} → {}",
                if changed.replaced { "replaced" } else { "added" },
                changed.state,
                dir.join(&changed.file).display()
            );
            print_clip_table(&dir)?;
            println!("\nsee it: tether ask");
            Ok(())
        }
        Action::UnsetClip { pet, state } => {
            let dir = crate::pack::resolve_pet(data, pet.as_deref())?;
            let changed = crate::pack::remove_clip(&dir, &state)?;
            println!("removed {} ({})", changed.state, changed.file);
            print_clip_table(&dir)?;
            Ok(())
        }
        Action::Clips { pet } => {
            let dir = crate::pack::resolve_pet(data, pet.as_deref())?;
            let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
            println!("{name}  ({})\n", dir.display());
            print_clip_table(&dir)?;
            Ok(())
        }
        Action::CharacterStudio => {
            let path = write_character_studio(data)?;
            println!("Character Studio (offline clip-pack authoring): {}", path.display());
            println!("  make a character prompt → animate it → drop the clips in → get pet.json");
            println!("  nothing is uploaded; the page makes no network requests.");
            open_path(&path);
            Ok(())
        }
        Action::Avatars => {
            let found = crate::avatar::list(data);
            if found.is_empty() {
                println!(
                    "no avatars yet — put .vrm/.glb/.gltf files in {}",
                    crate::avatar::avatar_dir(data).display()
                );
                println!("make a free human avatar offline with VRoid Studio, or an animal with Blender.");
                return Ok(());
            }
            println!(
                "{} avatar(s) in {}\n",
                found.len(),
                crate::avatar::avatar_dir(data).display()
            );
            for (path, summary) in found {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                match summary {
                    Ok(s) => {
                        let mark = if s.openable { "ok  " } else { "BLOCK" };
                        println!("  {mark} {name}");
                        println!(
                            "        {} — {}",
                            s.summary,
                            if s.humanoid { "humanoid rig" } else { "no humanoid rig" }
                        );
                        if s.findings > 0 {
                            println!(
                                "        {} finding(s); run `tether ask --audit` for detail",
                                s.findings
                            );
                        }
                    }
                    Err(e) => println!("  ERR  {name}\n        {e}"),
                }
            }
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
                println!("make one:  tether ask --new my-pet");
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

/// Prints every animation state a pet could have, and what it actually has.
///
/// A table, not a summary: the useful question is "which ones am I missing", and a list of
/// what exists cannot answer it.
fn print_clip_table(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // A sprite pet has no clips and never will; printing ten empty rows for it is noise.
    let is_sprite = std::fs::read_to_string(dir.join("pet.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| {
            v.get("clips")
                .and_then(|c| c.as_object())
                .map(|o| o.values().all(|f| f.as_str().is_some_and(str::is_empty)))
                .unwrap_or(true)
        })
        .unwrap_or(false);
    if is_sprite {
        println!("{} is a sprite companion — it has a single sheet, not clips.", dir.display());
        println!();
        println!("to get all ten states, author a clip pack — then add clips to it over time:");
        println!("  tether ask --character-studio          prompts and a drop zone, offline");
        println!("  tether ask --set-clip idle FILE        then walk, water, and the rest");
        println!("  tether ask --clips                     what you have, what is missing");
        return Ok(());
    }
    let inv = crate::pack::inventory(dir)?;
    let has = inv.iter().filter(|(_, c)| c.is_some()).count();
    println!(
        "{:<10} {:<10} {:>7} {:>8} {:>8}  file",
        "state", "status", "frames", "seconds", "size"
    );
    for (state, clip) in &inv {
        // Only `idle` is required to load a pack, so only `idle` may be labelled required.
        let required = state == "idle";
        match clip {
            Some(c) => println!(
                "{:<10} {:<10} {:>7} {:>8.1} {:>8}  {}",
                state,
                if required { "required" } else { "ok" },
                c.info.frames,
                c.info.duration_ms as f64 / 1000.0,
                format!("{:.2} MB", c.info.bytes as f64 / 1e6),
                c.file
            ),
            None => println!(
                "{:<10} {:<10} {:>7} {:>8} {:>8}  {}",
                state,
                if required { "missing" } else { "—" },
                "—",
                "—",
                "—",
                if required { "required to load" } else { "optional — add any time" }
            ),
        }
    }
    println!("\n{has} of {} states present.", inv.len());
    Ok(())
}

/// The clip-pack authoring page. Embedded, so `--character-studio` works with no network and no
/// assets on disk: one self-contained HTML file.
fn write_character_studio(data: &Path) -> std::io::Result<PathBuf> {
    const STUDIO: &str = include_str!("../gui/clip-studio.html");
    let dir = data.join("studio");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("character-studio.html");
    std::fs::write(&path, STUDIO)?;
    Ok(path)
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
    let pet_name = pet.as_ref().map(|p| p.name.clone()).unwrap_or_else(|| "tether".into());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_asset_is_embedded_and_offline() {
        assert!(include_str!("../assets/pet-studio.html").contains("Pet Studio"));
        // The clip studio must stay self-contained: a network request would break the promise
        // that nothing is uploaded and nothing is fetched.
        let cs = include_str!("../gui/clip-studio.html");
        assert!(cs.contains("Character Studio"));
        assert!(!cs.contains("https://"), "clip studio must not reference any URL");
        assert!(!cs.contains("http://"), "clip studio must not reference any URL");
    }

    #[test]
    fn studio_references_no_network_resources() {
        let html = include_str!("../assets/pet-studio.html");
        for banned in ["http://", "https://", "//cdn", "src=\"//"] {
            assert!(!html.contains(banned), "studio must not reference {banned}");
        }
        assert!(html.contains("data:image/png;base64,"), "art must be embedded");
    }

    #[test]
    fn new_then_list_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        run(
            d.path(),
            Action::New {
                name: "Test Pet".into(),
                frame_size: 32,
                body: "#c2331a".into(),
                accent: "#1f6b3a".into(),
            },
        )
        .unwrap();
        let (pets, bad) = pet::installed(&d.path().join("pets"));
        assert_eq!(pets.len(), 1);
        assert!(bad.is_empty());
        assert_eq!(pets[0].name, "Test Pet");
    }

    #[test]
    fn new_refuses_to_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let a = || Action::New {
            name: "Dup".into(),
            frame_size: 32,
            body: "#c2331a".into(),
            accent: "#1f6b3a".into(),
        };
        run(d.path(), a()).unwrap();
        assert!(run(d.path(), a()).is_err());
    }

    #[test]
    fn local_time_is_plausible() {
        let (h, m) = local_hour_minute();
        assert!(h < 24 && m < 60);
    }

    #[test]
    fn art_renders_nonempty() {
        assert!(!frame_art("Mochi", 0, false).is_empty());
        assert!(!frame_art("Mochi", 1, true).is_empty());
        assert!(!idle_art(0, true).is_empty());
    }
}
