# ADR 0006 — 3D avatars: vendor the renderer, ship parameters not art

**Status:** accepted
**Date:** 2026-11-13

## Context

The companion shipped as 2D pixel art. The request was explicit: the pet must look like a
**realistic 3D human or animal**, and it must be user-customisable or user-created.

Four constraints were already binding at that point:

1. **No network code.** Ever. It is the product's central claim and a test enforces it.
2. **The capture core stays small.** No new dependencies in the capture path.
3. **Everything works offline**, including first run.
4. **Downloaded content is untrusted input.**

The naive answer — "bundle a realistic model and render it" — fails twice. A believable human
avatar is 5–80 MB of rigged mesh, textures and clothing, usually licensed in a way that forbids
redistribution in an MIT repo. And a renderer is a large dependency that must be present offline.

## Decision

1. **Tether is the runtime, not the art vendor.** It loads user-supplied avatars in the two
   formats the ecosystem already produces: **VRM** (glTF + standard humanoid bones + expressions)
   for humans, **glTF/GLB** for everything else. VRoid Studio exports VRM for free, offline.
2. **Vendor three.js r187** (824 KB minified, MIT) into `gui/vendor/`, built from npm with
   esbuild. A tool that promises no network code cannot fetch its renderer from a CDN.
3. **Built-in characters are parameter sets, not meshes.** A normalized skeleton (`Species` +
   height, head ratio, shoulder width, limb radius + palette) builds the body at runtime. A few
   hundred bytes, no licence surface, infinitely adjustable.
4. **Three rendering paths, one optional component.** Procedural, glTF and VRM all live in
   `gui/`, never in the capture binary. `cargo build` produces the same 1.9 MB executable with or
   without 3D.
5. **Avatars are audited before load.** `src/avatar/audit.rs` reads header and JSON only — never
   geometry, never pixels.

## Consequences

**Good**

- Adding 3D changed the capture binary by **zero bytes**. The constraint held.
- A user can install a character they own, made in a free tool, with no network and no licence
  problem for this repo.
- Parameter-driven built-ins stay under a few KB and are trivially customisable (`sanitized()`
  clamps every field).
- Security posture improves rather than degrades: the audit refuses external `uri` references,
  flags metadata leaks, and reads image dimensions from headers so a decompression bomb is
  rejected before any allocator sees it.

**Costs**

- 824 KB of vendored JavaScript, plus build steps in `gui/vendor/README.md`. Accepted: it is
  separate from the binary, lazily loaded, and reproducible.
- The procedural characters are **stylised, not photoreal.** No amount of Rust makes a scaled
  sphere look like a person, and the documentation says so plainly rather than overselling.
- A VRM will not look identical across renderers (materials, outlines, toon shading assumptions).
- WebGL can be unavailable (remote desktops, headless VMs). The preview degrades visibly with an
  explanatory message and the sprite path remains available — *degrade visibly, never silently*.

## Alternatives rejected

| Alternative | Why not |
| --- | --- |
| Bundle one realistic GLB | Licence, repo size, and it locks in a character the user did not choose |
| Load three.js from a CDN | Directly violates the no-network invariant |
| Write a software rasteriser in Rust | Months of work to be worse than three.js, and it puts graphics in the capture core |
| Ship only 2D, explain the limit | The request was explicit; a genuine runtime for user avatars is a better answer than a polished refusal |
| Photoreal-ify the procedural human | Dishonest. It would still be a mannequin, and time spent there is time not spent making VRM loading excellent |

## Notes for the next person

The vendored bundle is built with `--format=iife --global-name=TetherCompanion` for the
single-file preview and `--format=esm` for module loading. An inline `<script type="module">`
cannot contain `export` statements, so the ESM bundle silently delivers nothing to a preview page
— that cost an afternoon, and it is why `gui/vendor/README.md` states the flag explicitly.
