# Pet format (`tether ask` / `.sppet`)

A pet is **two files in a folder**:

```
my-pet/
├── pet.json     the manifest
└── sprite.png   one horizontal spritesheet
```

That's it. No archive format to learn, no engine to install. Zip the folder and rename to
`.sppet` to share it. The format is deliberately boring so that a kid with a pixel-art tool, an
AI image generator, or a shell script can all produce one.

## sprite.png

A single horizontal strip of square frames, read left to right:

| Frames | Row | Meaning |
|---|---|---|
| 4 | 0 | **idle** — breathing, blinking. Loops forever. |
| 4 | 1 | **walk** — must loop seamlessly (frame 4 must lead back into frame 1). |
| 2 | 2 | **sleep** — eyes closed. Used after `sleep_after_min` idle. |
| 2 | 3 | **remind** — attention pose, shown while a reminder bubble is up. |
| 3 | 4 | **celebrate** — optional; played when you recover lost text. |

Frame size must be square and a power of two (16, 32, 48, 64, 96, 128). Rows are optional
after `idle`/`walk` — a 32×32 pet with only 8 frames is valid and charming.

```
sprite.png  (frame 32px, 4 columns)
┌────┬────┬────┬────┐
│ 1  │ 2  │ 3  │ 4  │  idle
├────┼────┼────┼────┤
│ 5  │ 6  │ 7  │ 8  │  walk
├────┼────┼────┼────┤
│ 9  │ 10 │    │    │  sleep
└────┴────┴────┴────┘
```

## pet.json

```json
{
  "name": "Mochi",
  "author": "you",
  "version": 1,
  "frame_size": 32,
  "scale": 2,
  "anchor": "bottom",
  "fps": { "idle": 4, "walk": 8, "sleep": 2, "remind": 6, "celebrate": 10 },
  "speech": {
    "water": ["water break!", "hydration check 💧", "your plants are drinking, join them"],
    "break": ["look away from the screen", "20-20-20: look 20 feet away for 20s", "stretch?"],
    "recovered": ["I caught that!", "nothing lost 🙂"]
  },
  "palette": ["#8b5cf6", "#f472b6"],
  "license": "CC0"
}
```

| Field | Required | Notes |
|---|---|---|
| `name` | yes | 1–32 chars, displayed in the UI |
| `version` | yes | must be `1`; future formats bump this and the loader refuses politely |
| `frame_size` | yes | px, must match the spritesheet width / columns |
| `scale` | no | integer 1–8, default 2. Never fractional — fractional scaling makes pixel art ugly |
| `fps` | no | per-animation frame rate, 1–30 |
| `speech` | no | reminder lines, per category; falls back to built-ins |
| `license` | no | shown in the pet picker; be honest about where the art came from |

## Safety rules the loader enforces

Pets are **content from the internet**, which means they are untrusted input. `pet::validate`
refuses, with a specific message, any pet that:

- has a `name` containing control characters, newlines, or right-to-left overrides (a bidi
  override can disguise what a reminder says);
- declares a `frame_size` that isn't a power of two in 16..=128, or `scale` outside 1..=8;
- 404s on the spritesheet, or has one whose dimensions don't fit `frame_size` (a cleverly
  crafted huge PNG is a decompression bomb);
- exceeds **2 MB** for the image or **16 KB** for the manifest;
- has speech lines longer than 120 characters or more than 24 lines per category (text becomes
  a UI-injection surface if it's unbounded);
- sets an `fps` outside 1..=30 (a 10000-fps pet is just a way to burn someone's battery).

## Authoring without drawing

Three paths, in order of effort:

```bash
tether ask --new my-pet --frame-size 32 --palette "#8b5cf6,#f472b6"
```

generates a valid starter pet (procedural sprite: a bouncing blob with eyes) that already
animates, so you can open `sprite.png` in any editor and paint over it. This is the fastest way
to a customisable pet: you're editing six squares, not learning a rig.

```bash
tether ask --studio        # local, offline authoring page: click cells, pick colours,
                              # export sprite.png + pet.json in one click
tether ask --list          # what's installed
tether ask --install ./friend-pet   # validate, check, copy into the pets dir
```

An AI-generated sheet works too: ask for "a 4×5 spritesheet, 64×64 px frames, flat background",
drop it in a folder with a `pet.json`, and `--install` will tell you exactly what's wrong if the
grid is off.

## Where pets live

```
<data dir>/pets/<name>/pet.json
<data dir>/pets/<name>/sprite.png
```

Deleting the folder removes the pet. Nothing about a pet is ever uploaded, and the pet itself
has no access to your rescues — it reads two numbers (pending reminders, recent recoveries)
through the same CLI you use.

## A pet can be 3D

A sprite is charming; a rigged 3D character is what most people picture when they ask for a
"realistic pet". A pet may carry **either** a sprite sheet, **or** a 3D model, **or both** —
adding `model` does not disturb the 2D path, and a pet without `model` behaves exactly as it
always has.

```jsonc
{
  "name": "Kai",
  "model": {
    "file": "kai.vrm",        // bare file name, beside pet.json
    "scale": 1.0,             // 0.25 .. 4.0
    "anchor": "bottom",       // bottom | center | top
    "y_offset": 0.0,          // -2.0 .. 2.0, for models whose origin is not at the feet
    "clips": {                // optional: which animation to play for each state
      "idle": "Idle", "walk": "Walk", "sleep": "Sleep",
      "remind": "Wave", "celebrate": "Jump"
    }
  }
}
```

### Which format?

| You have | Use | Made with |
| --- | --- | --- |
| A human character | `.vrm` | [VRoid Studio](https://vroid.com/en/studio) (free, offline), or any VRM shop |
| An animal or object | `.glb` / `.gltf` | Blender export, any glTF store, Mixamo rigs |
| Nothing yet | no `model` | The built-in procedural character |

`.vrm` is glTF with a standard humanoid bone set and expression data. That standard is what lets
Tether animate *your* character's head, eyes and gait without shipping your character.

### The rules (enforced before the file is opened)

- **Bare file name only.** `../x.glb`, `sub/x.glb` and `.hidden.glb` are refused. A manifest is
  downloaded content; a path traversal in it is the obvious attack.
- Extension must be `.vrm`, `.glb` or `.gltf`.
- **64 MB** ceiling, checked from `metadata` before any read.
- The header is verified by magic: `glTF` + version 2 for binary files, a JSON object for
  `.gltf`. Nothing is decoded at this point.
- `scale`, `anchor` and `y_offset` are bounds-checked.

A file that passes these is still **untrusted**. Before it is loaded, `src/avatar/audit.rs` reads
its header and JSON — never its geometry or pixels — and reports:

- **external `uri` references** → *blocked*. A model that points at `https://…` is a tracking
  pixel with a skeleton.
- **identifying metadata** (`asset.extras`, VRM meta fields) → *warned*, with the exact strings
  listed so you can decide.
- **embedded images** → dimensions read from the file header, so a decompression bomb is refused
  before a decoder sees it.
- **Draco / meshopt compression** → informational: decoded locally, no network involved.

Run `tether ask --check <file>` to see the report for any avatar before installing it.

### Built-in characters are parameters, not art

`tether ask --new Kai --species human` writes a manifest, not a mesh. The renderer builds the
body from a normalized skeleton — every part a fraction of one height value — so a built-in
character is a few hundred bytes and infinitely adjustable:

```jsonc
"avatar": {
  "species": "human",          // human | cat | dog | blob
  "height": 1.7,
  "head_ratio": 0.16,          // head diameter as a fraction of height
  "shoulder": 0.24,
  "limb": 0.055,
  "palette": { "skin": "#e8b58a", "hair": "#1b1410", "top": "#6d28d9" }
}
```

These are deliberately stylised. A procedural human will never be photoreal, and pretending
otherwise would be dishonest — if you want realism, rig a VRM; the runtime is here for it.

## The realistic path: clip packs

The characters people actually want look like this — AI-generated character art, animated into
short clips, delivered as **animated WebP with an alpha channel**. No rig, no engine, no GPU.
A pet can be built entirely from clips:

```jsonc
{
  "name": "Amma",
  "clips": {
    "idle":   "idle.webp",     // required
    "walk":   "walk.webp",     // required
    "remind": "wave.webp",
    "water":  "water.webp",
    "stretch":"stretch.webp",
    "eyes":   "eyes.webp",
    "celebrate": "happy.webp",
    "sleep":  "sleep.webp"
  }
}
```

A pet uses **one** kind of art — `sprite.png`, `clips`, or `model`. Saying so up front is kinder
than a manifest that half-works.

### States

| State | Required | When it plays |
| --- | --- | --- |
| `idle` | **yes** | at rest; breathing, blinking |
| `walk` | **yes** | wandering across the screen |
| `remind` | no | attention while a reminder is up |
| `water`, `stretch`, `eyes` | no | the matching reminder kind |
| `celebrate`, `happy`, `sad` | no | after a recovery, a good streak, a missed one |
| `sleep` | no | overnight |

Missing optional states degrade visibly — a pack with no `water` plays `remind`, and a pack with
neither plays `idle` — never a broken image.

### Validated before the renderer sees a frame

An animated image is a container whose header can claim anything. A 2 KB WebP can declare a
16000×16000 canvas and 100 000 frames; the decoder believes it and allocates 1 GB. So
`src/avatar/clips.rs` walks the container structure — `VP8X`/`ANIM`/`ANMF` for WebP, `acTL`/`fcTL`
for APNG — and computes the decode cost **without decoding anything**:

- canvas over **4096×4096** → refused as a decompression bomb, with the cost in the message;
- more than **20 000 frames** → refused;
- more than **12 MB** per clip, or **24 MB** per pack → refused;
- a **still image** where an animation is required → refused;
- a `walk` cycle with a finite loop count → refused (a walk that freezes is a bug you cannot see
  in a browser preview);
- bare file names only — `../x.webp` is refused, same rule as sprites and models.

### Making one

`tether ask --character-studio` opens an offline authoring page:

1. **Make the art.** It writes the image prompt and a matching animation prompt for every state —
   the prompts are the part that decides whether the character looks good.
2. **Build the pack.** Drop the clips in; it reads each header locally and shows size, fps, frame
   count, and whether the animation loops.
3. **Check and install.** It writes `pet.json`, flags a missing `idle`, and prints the install
   command.

Nothing is uploaded and the page makes no network requests — a test enforces the absence of any
URL in the file. Any tool works: image generators, image-to-video models, Blender, or hand-drawn
frames. Recommended export: **264×280, ~17 fps, alpha on, loop forever** — that is what the
reference companions ship and it comes to a few MB per character.
