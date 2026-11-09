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

use crate::pet;

pub enum Action {
    New {
        name: String,
        frame_size: u32,
        body: String,
        accent: String,
    },
    List,
    Install {
        path: PathBuf,
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
}
}









#[cfg(not(unix))]
fn local_offset_hours() -> i32 {
    // Windows: the GUI/companion reads real local time through the OS API; the terminal
    // companion uses UTC-based hours, and quiet hours are documented as approximate there.
    0
}
