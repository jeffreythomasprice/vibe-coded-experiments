---
name: pixel-sprites
description: Create art and code for this project's procedural pixel characters - generate source sheets with an image model, turn them into text stamps, and build renderers for new character kinds (humans, animals, monsters, robots, blobs). Use when asked for new or changed stamps, hair, hats, faces, body parts, creatures, or a new kind of character.
---

# Procedural pixel sprites

A character is drawn from two kinds of parts:

- **Shapes**, drawn by code: simple masses that vary in size and move (torso, hands, feet).
- **Stamps**, small text bitmaps: places where single pixels matter (face, hair, hat).

Generated images are source material. They are not game assets until converted
and cleaned up, and some are only useful as a visual reference.

Code is the source of truth. Read these before changing anything:

| File | Holds |
|---|---|
| `src/creature.rs` | `Facing`, `Anim`, `Kind`, dispatch between creature kinds, reach and shadows |
| `src/human.rs` | Human pose, proportions, part layout and draw order |
| `src/hound.rs` | Hellhound (quadruped): trot, body, legs, tail, animated fire |
| `src/stamp.rs` | Stamp text format and the list of bundled stamps |
| `src/pixbuf.rs` | Drawing, shading, outlining, mirroring of stamps |
| `src/palette.rs` | Materials and colour ramps |
| `src/main.rs` | Input, movement, screen bounds, shadow |
| `tools/stampify/src/main.rs` | Image-to-stamp converter |

## 1. Plan the parts

Before generating anything, write down for the character:

1. The parts, and which are shapes and which are stamps. Prefer a shape if the
   part moves a lot or should vary in size; prefer a stamp if it has features.
2. Each stamp's rough pixel size. The human head is 14x12 in a 32x40 sprite.
3. Which parts recolour independently (one material each).
4. Which views are needed. Normally south, north and side (facing **right**);
   west is the side view mirrored. A radially symmetric part may need only one.

For a new kind of character, confirm this breakdown with the user before
spending image generations on it.

## 2. Generate source sheets

Use the `generate-image` skill (it covers presets, result parsing and errors).
Settings that were used for the human:

- `--preset image-eval-skill` (check the user's config; it was Gemini)
- `--rewrite never`, so the long style text is sent unchanged
- `--copies 3`
- `-o art/raw/<kind>/<part>.png`, which overrides that skill's temp-directory
  default. `art/raw/` is gitignored. Use a new name rather than overwriting a
  sheet that stamps were made from.
- One sheet per part, several parts in parallel in one background command

Also generate one **reference** sheet of the whole character in all views. It
shows proportions and how the parts fit together.

### Prompt template

```
<sheet description>. <views, left to right>. <STYLE>
```

`STYLE`, used verbatim for hair, hat and reference:

```
Retro 16-bit pixel art, very low resolution with big square blocky pixels, hard edges, no anti-aliasing, no gradients, limited palette, flat solid pure magenta #FF00FF background, no text, no labels, no grid lines, no shadows. Three separate items in one horizontal row with wide magenta gaps between them, all the same scale.
```

Change the last sentence to match the number of views.

Descriptions that were used:

| Sheet | Text |
|---|---|
| hair | `A sprite sheet of one chibi game character hairstyle only: a short shaggy brown wig with bangs, no face, no head, no body; the face area is left magenta. Left: seen from the front. Middle: seen from the back. Right: seen from the side facing right.` |
| hat | `A sprite sheet of one chibi game character hat only: a red adventurer's cap with a small brim, no head, no body. Left: seen from the front. Middle: seen from the back. Right: seen from the side facing right.` |
| face | `A sprite sheet of two cute chibi game character faces, each a plain bald skin-colored oval head with only eyes, eyebrows and a tiny mouth, large simple features. Left: seen from the front. Right: seen from the side facing right.` |
| reference | `A character turnaround sprite sheet of one cute chibi villager with big head, brown hair, blue tunic, small detached floating hands and detached floating feet, top-down RPG style. Left: front view. Middle: back view. Right: side view facing right.` |

Prompting rules:

- Ask for **only the part**, and name what to leave out ("no head, no body").
- Never use magenta or pink-purple in the subject; it is keyed out.
- Items must not touch, or the converter cannot separate them.

For a new creature, keep the structure and swap the subject, for example:
`A sprite sheet of one cute robot head only: boxy metal head with a single round
visor eye and a short antenna, no body. Left: ... front. Middle: ... back.
Right: ... side facing right.`

Look at every candidate with `Read` and pick by silhouette: the one with the
clearest shape and flattest colours survives downscaling best. Check that the
side view really faces right.

For the hellhound (`art/raw/hellhound/`), both sheets were too fine to convert
(about 25 source pixels per head), so its head and flame stamps were drawn by
hand after `head2` and `reference0`.

### What to expect (from the first run, one sample per part)

| Part | Chosen | Result |
|---|---|---|
| hat | `hat2` | Converted well; trimmed shorter and cleaned by hand |
| hair | `hair1` | Roughly 55 source "pixels" wide; noisy at 16, so redrawn by hand |
| face | `face0` | Converter kept the head outline; redrawn by hand |
| reference | - | About 14 source pixels per head, close to game scale; looked at, not converted |

The generator draws finer than asked. Naming an exact coarse size ("a 16x16
pixel sprite") has not been tried.

## 3. Convert

```bash
cargo build --release --manifest-path tools/stampify/Cargo.toml
S=tools/stampify/target/release/stampify
# stampify <image> <out-dir> <part> <material> <width> <view,view,...>
$S art/raw/hair1.png art/converted hair hair 16 south,north,side
$S art/raw/hat2.png  art/converted hat  hat  16 south,north,side
$S art/raw/face0.png art/converted face face 14 south,side
```

- Always convert into `art/converted/`, **never straight into
  `assets/stamps/`**, which holds hand-edited files that would be overwritten.
- `width` is the first view's width in stamp pixels; other views share its scale.
- The converter keys out magenta, takes the most common colour per cell, and
  assigns shades 1-4 by brightness. Its anchor is just the stamp centre.
- Material `face` keeps only dark (`k`) and white (`w`) pixels. Head outlines
  come through too and must be deleted.
- The converter accepts any material name; `src/stamp.rs` rejects unknown ones.

If a result is unreadable, hand-write the stamp using the sheet as reference.
That is a normal outcome, not a failure of the process. Tell the user which
stamps were converted and which were redrawn.

## 4. Author and clean up stamps

```text
; Where it came from. Faces right.
material=hair anchor=8,8
.....333333.....
...3344444433...
```

| Char | Meaning |
|---|---|
| `.` | transparent |
| `1`-`4` | shade of the header's material, darkest to lightest |
| `e`, `E` | dim and bright eye |
| `n`, `N` | dark and light horn |
| `r` | inside of the mouth |
| `#` | outline colour |
| `w`, `k` | white, near-black |

- All rows must be the same width.
- The anchor is the stamp position pinned to the part's centre. It is a pixel
  boundary, so an even-width stamp with anchor `x = width/2` is centred and
  mirrors cleanly.
- Human head: `x` from -7 to +6 and `y` from -6 to +5 around its centre. Eyes
  start at `y = 0`. Anchors in use: hair `8,8`, hat `8,9`, front face `5,0`,
  side face `-2,0`.
- Do not draw an outer `#` border: `outline_pass` adds it. Use `1` for inner
  edges such as bangs against skin.
- Use shades, not fixed colours, so palettes can recolour the part.
- Light comes from above: highlights high, shade `1` along lower edges.
- Remove isolated single pixels; at this size they read as noise.

### Adding a stamp to the app

1. Save it as `assets/stamps/<part>_<view>.txt`.
2. In `src/stamp.rs`, add it to `Stamps` and `Stamps::load` (use `Views` for a
   south/north/side set).
3. Blit it in the kind's module, flipping when facing west.
4. For several variants of one part, hold a list and pick one in
   the kind's `random`. Adding an `rng.pick` changes what later picks return,
   so add new ones last to keep existing seeds looking the same.
5. For a new material, add it to `Material`, give it ramps and a `Palette` field
   in `src/palette.rs`, and add its name to `parse_material`.

## 5. New kinds of character

### What carries over

- Reusable as is: `rng`, `pixbuf`, the stamp format, the text converter.
- Reusable with small changes: `palette` (materials are named for humans),
  `Facing` and `Facing::project`, the per-part `layer` then composite pattern.
- Per kind: `Part`, `Pose`, the size and origin constants, `render` and `layer`.
  `human.rs` and `hound.rs` are the two examples.

### How to structure it

Give each kind its own module with its own parts, body, pose and stamps, then
add it to `Kind` and `Creature` in `src/creature.rs`. The `t` key cycles through
`Kind::ALL`, and `h` calls `toggle_extra`. Keep existing kinds' behaviour and
tests unchanged. Do not force new creatures into another kind's fields.

### Layout rules

- Give each part a body-relative position (`right`, `forward`, height) and get
  screen position and draw order from `project`. Avoid per-facing special cases
  where the projection can do the work.
- The human shortcuts do not generalise: its torso is always order 0 and head 1.
  A head or tail that sticks out forward or back **must** take its order from
  projected depth, so a quadruped's head goes behind its body facing north.
- Long bodies need a different torso width per facing (long in side view, short
  head-on). Add a height offset for depth in front and back views, or the front
  and rear parts stack on one spot.
- Each part gets its own shade and outline pass so overlaps stay readable.
  Parts that should merge (head, face, hair) share one layer.
- Use even widths for centred shapes.

### Starting points (untested ideas)

| Kind | Shapes | Stamps | Motion |
|---|---|---|---|
| Slime / blob | one ellipse | eyes, mouth | squash and stretch, hop |
| Robot | boxy torso, claws, treads or feet | head with visor, antenna | stiff 1px steps, blinking light, tread cycle |
| Flyer / ghost | body, tapered tail | face, wings (2-3 frames) | hover bob, shadow stays on the ground |
| Serpent | chain of circles following the head | head per view | wave offset along the chain |
| Many-legged | body, 6-8 small legs | eyes, mandibles | alternating leg groups |

New shapes (triangle, line, circle chain) belong in `pixbuf.rs` with a test.

### Check outside the character module

- Sprite size, origin and ground line if the creature is bigger than 32x40.
- In `src/creature.rs`: `reach` (movement bounds) and `shadow` per kind.
  Airborne creatures need the shadow separate from height.

## 6. Verify

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo clippy --target wasm32-unknown-unknown -- -D warnings
trunk build
```

`every_kind_stays_inside_its_sprite_and_reach` covers every entry in
`Kind::ALL`; make its time range long enough for a new kind's slowest cycle.

Tests cannot judge the art. Run `trunk serve` (port 8000) and look in the
browser at every facing, each walk frame and several seeds. Held keys can be
simulated with `document.dispatchEvent(new KeyboardEvent('keydown', {key:
'ArrowRight'}))` and a matching `keyup`; copying crops of the canvas onto an
overlay canvas gives a side-by-side contact sheet. Check that:

- the silhouette reads at a glance and differs between facings
- near parts cover far parts, in both east and west
- limbs stay visible during the walk, especially in profile (the weakest view
  of the human)
