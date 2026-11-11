//! Header-only audit of **animated image clip packs**.
//!
//! The realistic-looking companions people actually want are not 3D models at all: they are
//! AI-generated character art animated into short clips and delivered as **animated WebP**
//! (or APNG/GIF). Full-frame sequences with a real alpha channel. That is a good format — it
//! gives you Pixar-quality art with no rig, no engine and no GPU.
//!
//! It is also an *untrusted input with a decompression bomb built in*. A 2 KB animated WebP can
//! declare a 16000×16000 canvas and 100 000 frames; the decoder believes the header, allocates,
//! and you are out of memory before a single pixel is drawn. So this module reads the container
//! structure — RIFF/VP8X/ANIM/ANMF for WebP, `acTL`/`fcTL` for APNG, block descriptors for GIF —
//! and computes the decode cost **without decoding anything**.
//!
//! The rules are the same as [`super::audit`]: a security check must never be the thing that
//! gets you owned. Read the header, state the cost, refuse the implausible.

use std::path::Path;

/// An animated clip may be big — high-quality character art is — but not unbounded.
pub const MAX_CLIP_BYTES: u64 = 12 * 1024 * 1024;
/// Canvas cap. 4096×4096 is 16.7 Mpx, ~67 MB decoded at RGBA8: plenty for a desktop pet.
pub const MAX_CANVAS_PIXELS: u64 = 4096 * 4096;
/// Frame cap. At 30 fps this is ~11 minutes of animation; nothing legitimate needs more.
pub const MAX_FRAMES: u32 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// RIFF/WEBP with a VP8X animation chunk.
    AnimatedWebp,
    /// RIFF/WEBP without animation (a poster frame, which is what `.still.webp` usually is).
    StillWebp,
    /// PNG with an `acTL` animation control chunk.
    Apng,
    Png,
    Gif,
}

impl Container {
    pub fn label(&self) -> &'static str {
        match self {
            Container::AnimatedWebp => "animated WebP",
            Container::StillWebp => "still WebP",
            Container::Apng => "animated PNG (APNG)",
            Container::Png => "still PNG",
            Container::Gif => "GIF",
        }
    }

    pub fn is_animated(&self) -> bool {
        matches!(self, Container::AnimatedWebp | Container::Apng | Container::Gif)
    }

    /// Extensions this container is allowed to use in a pet folder.
    pub fn extensions(&self) -> &'static [&'static str] {
        match self {
            Container::AnimatedWebp | Container::StillWebp => &["webp"],
            Container::Apng | Container::Png => &["png", "apng"],
            Container::Gif => &["gif"],
        }
    }
}

/// What the header said. Everything here is read from a few hundred bytes at most.
#[derive(Debug, Clone)]
pub struct ClipInfo {
    pub container: Container,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// Total playback time in milliseconds, when the container states it per frame.
    pub duration_ms: u64,
    pub has_alpha: bool,
    /// Loop count, 0 = forever (the usual for a pet).
    pub loops: u16,
    pub bytes: u64,
}

impl ClipInfo {
    /// Worst-case decoded size of a single frame, in bytes (RGBA8).
    pub fn frame_bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * 4
    }

    /// Worst-case *retained* memory: one RGBA canvas plus one spare for compositing.
    pub fn decode_bytes(&self) -> u64 {
        self.frame_bytes() * 2
    }

    pub fn fps(&self) -> f32 {
        if self.frames == 0 || self.duration_ms == 0 {
            return 0.0;
        }
        (f64::from(self.frames) * 1000.0 / self.duration_ms as f64) as f32
    }

    pub fn summary(&self) -> String {
        if self.container.is_animated() {
            format!(
                "{} · {:.1}s · {:.0} fps · {:.1} MB decoded",
                self.container.label(),
                self.duration_ms as f64 / 1000.0,
                self.fps(),
                self.decode_bytes() as f64 / 1e6
            )
        } else {
            format!("{} {}×{}", self.container.label(), self.width, self.height)
        }
    }
}

/// A reason to refuse, in words a user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipProblem {
    pub what: String,
}

impl std::fmt::Display for ClipProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.what)
    }
}

/// Reads the container header. Never decodes pixels, never allocates proportional to content.
pub fn sniff(path: &Path) -> Result<ClipInfo, ClipProblem> {
    let bytes = std::fs::metadata(path)
        .map_err(|e| ClipProblem { what: format!("cannot read: {e}") })?
        .len();
    if bytes == 0 {
        return Err(ClipProblem { what: "is empty".into() });
    }
    if bytes > MAX_CLIP_BYTES {
        return Err(ClipProblem { what: format!("{bytes} bytes, limit is {MAX_CLIP_BYTES}") });
    }
    // A clip header is small; 1 MB covers a WebP's whole chunk table many times over.
    let head = read_head(path, 1024 * 1024)?;

    if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return sniff_webp(&head, bytes);
    }
    if head.len() >= 8 && head[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return sniff_png(&head, bytes);
    }
    if head.len() >= 6 && (&head[..6] == b"GIF87a" || &head[..6] == b"GIF89a") {
        return sniff_gif(&head, bytes);
    }
    Err(ClipProblem { what: "is not a WebP, PNG/APNG or GIF".into() })
}

fn read_head(path: &Path, n: usize) -> Result<Vec<u8>, ClipProblem> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| ClipProblem { what: e.to_string() })?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf).map_err(|e| ClipProblem { what: e.to_string() })?;
    buf.truncate(got);
    Ok(buf)
}

fn le24(b: &[u8], at: usize) -> u32 {
    u32::from(b[at]) | (u32::from(b[at + 1]) << 8) | (u32::from(b[at + 2]) << 16)
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from(b[at]) | (u16::from(b[at + 1]) << 8)
}

/// RIFF container: walk the chunk table, read `VP8X` (canvas + flags) and count `ANMF` frames.
fn sniff_webp(head: &[u8], bytes: u64) -> Result<ClipInfo, ClipProblem> {
    let mut width = 0u32;
    let mut height = 0u32;
    let mut animated = false;
    let mut alpha = false;
    let mut frames = 0u32;
    let mut duration_ms = 0u64;
    let mut loops = 1u16;
    let mut saw_vp8x = false;

    let mut pos = 12usize; // after RIFF....WEBP
    while pos + 8 <= head.len() {
        let kind = &head[pos..pos + 4];
        let len = u32::from_le_bytes([head[pos + 4], head[pos + 5], head[pos + 6], head[pos + 7]])
            as usize;
        let body = pos + 8;
        match kind {
            b"VP8X" if body + 10 <= head.len() => {
                saw_vp8x = true;
                let flags = head[body];
                alpha = flags & 0x10 != 0;
                animated = flags & 0x02 != 0;
                width = le24(head, body + 4) + 1;
                height = le24(head, body + 7) + 1;
            }
            b"ANIM" if body + 6 <= head.len() => {
                loops = le16(head, body + 4);
            }
            b"ANMF" if body + 16 <= head.len() => {
                frames += 1;
                // 16-byte frame header: x(3) y(3) w-1(3) h-1(3) duration(3) flags(1)
                duration_ms += u64::from(le24(head, body + 12));
            }
            b"VP8 " => {
                // Simple lossy still WebP: dimensions live in the frame header.
                if !saw_vp8x && body + 10 <= head.len() {
                    let mut p = body;
                    if p + 3 <= head.len()
                        && head[p] == 0x9d
                        && head[p + 1] == 0x01
                        && head[p + 2] == 0x2a
                    {
                        p += 3;
                        if p + 4 <= head.len() {
                            width = u32::from(le16(head, p)) & 0x3fff;
                            height = u32::from(le16(head, p + 2)) & 0x3fff;
                        }
                    }
                }
            }
            // VP8L always carries an alpha channel; dimensions are packed into 32 bits.
            b"VP8L" if !saw_vp8x && body + 5 <= head.len() && head[body] == 0x2f => {
                let bits = u32::from_le_bytes([
                    head[body + 1],
                    head[body + 2],
                    head[body + 3],
                    head[body + 4],
                ]);
                width = (bits & 0x3fff) + 1;
                height = ((bits >> 14) & 0x3fff) + 1;
                alpha = true;
            }
            _ => {}
        }
        // Chunks are padded to an even length.
        pos = body + len + (len & 1);
        if kind == b"ANMF" && frames > MAX_FRAMES {
            // Stop counting rather than walking a hostile chunk table to the end. Everything we
            // report is therefore a lower bound, which is the safe direction to be wrong in.
            break;
        }
    }

    if width == 0 || height == 0 {
        return Err(ClipProblem { what: "has no readable canvas size".into() });
    }
    // Trust the frame count over the flag: a VP8X can advertise animation while carrying no
    // ANMF chunk at all, and calling that "animated" would report a 0-frame clip as playable.
    let container =
        if animated && frames > 0 { Container::AnimatedWebp } else { Container::StillWebp };
    let info =
        ClipInfo { container, width, height, frames, duration_ms, has_alpha: alpha, loops, bytes };
    check_budget(&info)?;
    Ok(info)
}

/// PNG: read IHDR for dimensions, and `acTL` for the APNG frame count.
fn sniff_png(head: &[u8], bytes: u64) -> Result<ClipInfo, ClipProblem> {
    if head.len() < 33 {
        return Err(ClipProblem { what: "PNG header is truncated".into() });
    }
    let width = u32::from_be_bytes([head[16], head[17], head[18], head[19]]);
    let height = u32::from_be_bytes([head[20], head[21], head[22], head[23]]);
    let color_type = head[25];
    let has_alpha = matches!(color_type, 4 | 6) || head[28] != 0; // tRNS present

    let mut frames = 0u32;
    let mut duration_ms = 0u64;
    let mut loops = 1u16;
    let mut animated = false;
    let mut pos = 8usize;
    // Walk the chunk table: length(4) type(4) data(len) crc(4)
    while pos + 12 <= head.len() {
        let len =
            u32::from_be_bytes([head[pos], head[pos + 1], head[pos + 2], head[pos + 3]]) as usize;
        let kind = &head[pos + 4..pos + 8];
        let body = pos + 8;
        match kind {
            b"acTL" if body + 8 <= head.len() => {
                animated = true;
                frames = u32::from_be_bytes([
                    head[body],
                    head[body + 1],
                    head[body + 2],
                    head[body + 3],
                ]);
                loops = u16::try_from(u32::from_be_bytes([
                    head[body + 4],
                    head[body + 5],
                    head[body + 6],
                    head[body + 7],
                ]))
                .unwrap_or(u16::MAX);
            }
            b"fcTL" if body + 26 <= head.len() => {
                // delay_num (2) at offset 20, delay_den (2) at 22
                let num = u32::from(le16(head, body + 20));
                let den = u32::from(le16(head, body + 22)).max(1);
                duration_ms += u64::from(num) * 1000 / u64::from(den);
            }
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        pos = body + len + 4;
    }

    let container = if animated { Container::Apng } else { Container::Png };
    let info = ClipInfo { container, width, height, frames, duration_ms, has_alpha, loops, bytes };
    check_budget(&info)?;
    Ok(info)
}

/// GIF: count Image Descriptor blocks. Block walking needs the whole file, so this reads up to
/// the 1 MB head and reports what it found — enough to refuse an implausible frame count.
fn sniff_gif(head: &[u8], bytes: u64) -> Result<ClipInfo, ClipProblem> {
    if head.len() < 13 {
        return Err(ClipProblem { what: "GIF header is truncated".into() });
    }
    let width = u32::from(le16(head, 6));
    let height = u32::from(le16(head, 8));
    let flags = head[10];
    let mut loops = 0u16;
    if flags & 0x80 != 0 {
        let gct = 3 * (1usize << (usize::from(flags & 0x07) + 1));
        let mut p = 13 + gct;
        // Look for a Netscape application extension (loop count).
        while p + 14 < head.len() {
            if head[p] == 0x21 && head[p + 1] == 0xFF {
                if head[p + 3..p + 14].starts_with(b"NETSCAPE2.0") && p + 18 < head.len() {
                    loops = le16(head, p + 16);
                }
                break;
            }
            if head[p] == 0x21 {
                p += 2 + 1 + usize::from(head[p + 2]);
            } else {
                break;
            }
        }
    }
    // Frame count: image separators seen in the head. A GIF is not transparently sampled, so
    // this is a lower bound — reported as such rather than guessed.
    let frames = head.iter().filter(|b| **b == 0x2C).count().min(MAX_FRAMES as usize) as u32;
    let info = ClipInfo {
        container: Container::Gif,
        width,
        height,
        frames,
        duration_ms: 0,
        has_alpha: flags & 0x01 != 0,
        loops,
        bytes,
    };
    check_budget(&info)?;
    Ok(info)
}

/// The budget checks. Refusing here is the whole point of reading headers instead of pixels.
fn check_budget(info: &ClipInfo) -> Result<(), ClipProblem> {
    let pixels = u64::from(info.width) * u64::from(info.height);
    if pixels > MAX_CANVAS_PIXELS {
        return Err(ClipProblem {
            what: format!(
                "canvas is {}×{} ({:.1} Mpx); the limit is {:.1} Mpx. A file that large is a \
                 decompression bomb, not a desktop pet.",
                info.width,
                info.height,
                pixels as f64 / 1e6,
                MAX_CANVAS_PIXELS as f64 / 1e6
            ),
        });
    }
    if info.frames > MAX_FRAMES {
        return Err(ClipProblem {
            what: format!("at least {} frames; the limit is {MAX_FRAMES}", info.frames),
        });
    }
    if info.width == 0 || info.height == 0 {
        return Err(ClipProblem { what: "has a zero dimension".into() });
    }
    Ok(())
}

/// Which animation states a clip pack should provide. Only `idle` is required to load: a pack is
/// assembled over days and across accounts, so demanding more on day one would make the intended
/// workflow impossible. `walk` is the strongly-recommended second one — without it the pet stands
/// still — and the rest light up the matching behaviour if present.
pub const CLIP_STATES: &[(&str, bool)] = &[
    ("idle", true),
    ("walk", false),
    ("remind", false),
    ("water", false),
    ("stretch", false),
    ("eyes", false),
    ("celebrate", false),
    ("happy", false),
    ("sad", false),
    ("sleep", false),
];

pub fn is_clip_state(name: &str) -> bool {
    CLIP_STATES.iter().any(|(k, _)| *k == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, data: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    /// Minimal animated WebP: RIFF header, VP8X with the animation flag, ANIM, N × ANMF.
    fn animated_webp(w: u32, h: u32, frames: u32, ms_per_frame: u32) -> Vec<u8> {
        let mut chunks = Vec::new();
        // VP8X
        let mut vp8x = vec![0x02 | 0x10]; // animation + alpha
        vp8x.extend_from_slice(&[0, 0, 0]);
        vp8x.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
        vp8x.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
        chunks.extend_from_slice(b"VP8X");
        chunks.extend_from_slice(&(vp8x.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&vp8x);
        // ANIM
        let anim = [0u8, 0, 0, 0, 0, 0];
        chunks.extend_from_slice(b"ANIM");
        chunks.extend_from_slice(&(anim.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&anim);
        // ANMF xN (16-byte frame header + 1 byte payload to be structurally honest)
        for _ in 0..frames {
            let mut f = vec![0u8; 12];
            f.extend_from_slice(&ms_per_frame.to_le_bytes()[..3]);
            f.push(0); // flags — exactly 16 bytes, like a real ANMF header
            chunks.extend_from_slice(b"ANMF");
            chunks.extend_from_slice(&(f.len() as u32).to_le_bytes());
            chunks.extend_from_slice(&f);
        }
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
        out.extend_from_slice(b"WEBP");
        out.extend_from_slice(&chunks);
        out
    }

    #[test]
    fn animated_webp_frame_count_and_timing_are_read() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "walk.webp", &animated_webp(320, 480, 18, 66));
        let info = sniff(&p).unwrap();
        assert_eq!(info.container, Container::AnimatedWebp);
        assert_eq!((info.width, info.height), (320, 480));
        assert_eq!(info.frames, 18);
        assert_eq!(info.duration_ms, 18 * 66);
        assert!(info.has_alpha, "alpha flag is set");
        assert!((info.fps() - 15.15).abs() < 0.1, "got {}", info.fps());
    }

    #[test]
    fn a_huge_canvas_is_refused_before_any_decode() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "bomb.webp", &animated_webp(16000, 16000, 2, 100));
        let err = sniff(&p).unwrap_err();
        assert!(err.what.contains("decompression bomb"), "{err}");
        assert!(err.what.contains("Mpx"));
    }

    #[test]
    fn an_absurd_frame_count_is_refused() {
        let d = tempfile::tempdir().unwrap();
        // 30 000 frames exceeds MAX_FRAMES. The file is small; the *cost* is not.
        let p = write(d.path(), "long.webp", &animated_webp(64, 64, 30_000, 33));
        let err = sniff(&p).unwrap_err();
        assert!(err.what.contains("frames"), "{err}");
        assert!(err.what.contains("at least"), "a truncated window must not overstate: {err}");
    }

    #[test]
    fn decode_cost_is_reported_honestly() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "a.webp", &animated_webp(1024, 1024, 10, 40));
        let info = sniff(&p).unwrap();
        // 1024*1024*4 = 4 MiB per canvas, two canvases.
        assert_eq!(info.frame_bytes(), 4 * 1024 * 1024);
        assert_eq!(info.decode_bytes(), 8 * 1024 * 1024);
        assert!(info.summary().contains("MB decoded"));
    }

    #[test]
    fn a_still_webp_is_not_reported_as_animated() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "still.webp", &animated_webp(200, 200, 0, 0));
        let info = sniff(&p).unwrap();
        assert_eq!(info.container, Container::StillWebp);
        assert!(!info.container.is_animated());
    }

    #[test]
    fn apng_is_detected_with_its_frame_count() {
        let d = tempfile::tempdir().unwrap();
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        // IHDR
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&256u32.to_be_bytes());
        png.extend_from_slice(&256u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0, 0, 0, 0]);
        // acTL: 12 frames, loop forever
        png.extend_from_slice(&8u32.to_be_bytes());
        png.extend_from_slice(b"acTL");
        png.extend_from_slice(&12u32.to_be_bytes());
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(&[0, 0, 0, 0]);
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0, 0, 0, 0]);
        let p = write(d.path(), "walk.png", &png);
        let info = sniff(&p).unwrap();
        assert_eq!(info.container, Container::Apng);
        assert_eq!(info.frames, 12);
        assert_eq!(info.loops, 0);
    }

    #[test]
    fn a_png_without_actl_is_a_still() {
        let d = tempfile::tempdir().unwrap();
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&64u32.to_be_bytes());
        png.extend_from_slice(&32u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0; 4]);
        png.extend_from_slice(&0u32.to_be_bytes());
        png.extend_from_slice(b"IEND");
        png.extend_from_slice(&[0; 4]);
        let info = sniff(&write(d.path(), "a.png", &png)).unwrap();
        assert_eq!(info.container, Container::Png);
        assert_eq!((info.width, info.height), (64, 32));
        assert!(info.has_alpha);
    }

    #[test]
    fn something_that_is_not_an_image_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = write(d.path(), "x.webp", b"<html>not an image</html>");
        assert!(sniff(&p).unwrap_err().what.contains("not a WebP"));
    }

    #[test]
    fn an_empty_file_is_refused() {
        let d = tempfile::tempdir().unwrap();
        assert!(sniff(&write(d.path(), "e.webp", b"")).unwrap_err().what.contains("empty"));
    }

    #[test]
    fn the_state_whitelist_covers_a_full_pet() {
        for (state, required) in CLIP_STATES {
            assert!(is_clip_state(state));
            if *required {
                assert_eq!(*state, "idle", "only idle may be required to load a pack");
            }
            assert!(*required == (*state == "idle"), "{state} required flag disagrees with pet.rs");
        }
        assert!(!is_clip_state("evil"));
        assert!(!is_clip_state("../etc/passwd"));
    }
}
