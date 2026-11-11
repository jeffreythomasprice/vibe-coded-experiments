# Materials, textures and UVs

How a face gets its look, from the scene file to the viewport. Written for people changing the code and for agents using the ops (CLI or MCP).

## The model in one paragraph

A **material** is a document object (id prefix `t`, e.g. `t42`) with a unique **name**, an optional **image**, and a few properties. A face refers to a material **by name** (`attrs.material`). The face also carries **UV attributes** that say how the image lies on it. A face whose name matches no material still loads and draws a flat colour hashed from the name; `check.validate` warns about it. The empty name means "no material" (grey).

## Material record

| Field | Type | Default | Viewport | Notes |
|---|---|---|---|---|
| `name` | string | | | Unique in the document; faces use it. |
| `texture` | image source | none | draws it | See below. |
| `color` | `[r, g, b, a]`, 0..1 | `[1,1,1,1]` | multiplies the image; the flat colour with no image; alpha < 1 draws translucent | |
| `emissive` | `[r, g, b]` | black | adds light regardless of shading | Multiplied by `emissive_strength`. |
| `emissive_strength` | number ≥ 0 | 0 | | 0 turns emission off. |
| `roughness`, `metallic` | 0..1 | 1, 0 | not shaded | Stored for the game or exporter and shown by `material.get`. |
| `filter` | `linear` / `nearest` | `linear` | sampler | `nearest` keeps pixel art crisp. |
| `double_sided` | bool | false | none | Stored only: the viewport never culls faces. |
| `unlit` | bool | false | skips the editor's Lambert shading | |
| `nodraw` | bool | false | drawn at most 35% opaque | For surfaces the game does not draw. |

Translucent faces (alpha < 1, `nodraw`) draw in a second pass after the opaque ones, with depth testing but no depth writes and **no sorting**, so overlapping translucent faces can blend in the wrong order.

## Image sources

A material's image is stored one of three ways. All three are valid in the file, over the ops and in the MCP.

| Source | In the file | Meaning |
|---|---|---|
| Absolute path | `texture = { path = "/art/brick.png" }` | The file at that path. |
| Relative path | `texture = { path = "../tex/brick.png" }` | Relative to the **directory of the scene file**. Needs a saved scene to resolve. |
| Embedded | `texture = { format = "png", base64 = "iVBOR…" }` | The image file's bytes, base64, in the scene. |

Formats are png, jpeg, tga and bmp (the file extension picks the format for paths; embedded images say it). Images larger than 4096 on a side are scaled down for display; the size used for UV maths is always the stored image's.

Rules worth knowing:

- `material.create` / `material.set` take a **texture input**: `{source: "file", path, store}` or `{source: "base64", format, data}`. For a file, `store` is `absolute`, `relative` or `embed`; the default is `relative` once the scene is saved, otherwise `absolute`. The file must exist and decode, or the op fails. Relative `path` arguments resolve against the caller's working directory (the server rewrites them for each connection) and are stored relative to the scene.
- `store: relative` on an unsaved scene is an `InvalidArgument` with the hint to `file.save` first.
- **Saving to another directory** (`file.save` with a new path) rewrites every relative texture path so it still points at the same file. The history is rewritten too, so undo stays correct.
- `material.embed` turns file textures into embedded ones, making the scene self-contained.
- An image that cannot be read does not stop a scene from loading. `material.list` / `material.get` report `texture.error`, `check.validate` reports `texture_unreadable` (an error), and the viewport draws a magenta/dark checker with a screenshot warning.
- Embedded images make scene files large and diffs noisy; prefer relative paths for anything under version control.

## UV attributes on a face

```toml
{ id = 1, plane = [...], material = "base/brick", offset = [8, 0], scale = [2, 2], rotation = 15, u_axis = [1, 0, 0], v_axis = [0, -1, 0] }
```

| Field | Unit | Default | Meaning |
|---|---|---|---|
| `offset` | texels | `[0, 0]` | Shifts the image. |
| `scale` | world units per texel | `[1, 1]` | Bigger numbers make the image larger on the face. Negative mirrors. |
| `rotation` | degrees | 0 | Turns the image counter-clockwise as seen from outside the face. |
| `u_axis`, `v_axis` | world vectors | derived | Optional explicit axes. Both or neither. |

The mapping from a world point `p` on the face to texels is

```
texel.u = dot(p, U') / scale.x + offset.x
texel.v = dot(p, V') / scale.y + offset.y
```

where `U'`, `V'` are the axes rotated by `rotation` about the face normal. Dividing by the image size gives the 0..1 `uv` a renderer samples (the image repeats outside 0..1). Row 0 of the image is at the top, so `v` grows downwards in the image. A material without an image counts as 64×64 texels.

**Paraxial axes** (no `u_axis`/`v_axis`): Quake-style, chosen by the dominant axis of the face normal. Floors and ceilings use U = +X, V = −Y; walls facing ±X use U = +Y, V = −Z; walls facing ±Y use U = +X, V = −Z. Rotation turns around that dominant axis. Opposite faces use the same axes, so an image reads mirrored on the back of a wall: that is the convention.

**Explicit axes** (Valve-220 style): used when the axes must follow a slanted or rotated face. They are written by texture lock, by `face.align_uv` reset to `face_aligned`, and by wrapping. Their lengths do not matter in the file; the writer stores unit vectors and puts the size in `scale`. Faces with explicit axes rotate about their own normal.

`face.get_uv` prints the numbers: the effective axes, each corner's texel and uv, and the texel bounds. Use it to check an alignment instead of squinting at a screenshot.

## Texture lock

Moving geometry normally should not slide textures off it, so the ops re-fit the UV attributes of each face from where its corners go. It is **on by default** and applies to:

- `transform.translate`, `transform.rotate`, `transform.scale`, `transform.flip` (`texture_lock: false` turns it off per call);
- `vertex.translate` and `face.translate`, for brushes and meshes;
- duplicating and arraying (copies keep their alignment).

How it works (`editor_geom::uv`): for a face, take corner pairs (position before, position after), look up each old corner's texel with the old attributes, and solve for the mapping that gives the same texel at the new corner. A face keeps its original parametrisation (paraxial, same scale and rotation) whenever only the offset needs to change, so a plain translation just edits `offset`; otherwise it gets explicit axes. When some corners did not move and three of them span the face (dragging one vertex of a quad), those stay put and the moved corner slides over the image; otherwise the best-conditioned three moved corners are used. A mirror needs no special case: the axes mirror with the geometry.

Faces created by splitting (a vertex pulled off a cube's corner) take the attributes of the face they split from, then lock to it. Clip, CSG and conversions keep their existing rules (new faces copy a neighbour's attributes); they do not re-fit.

The GUI's **Tex lock** toolbar toggle sends `texture_lock` on every drag it makes.

## Aligning faces

| Op | Use |
|---|---|
| `face.set_uv {faces, offset?, scale?, rotation?, mode?}` | Set values, or with `mode: "relative"` add to the offset, multiply the scale and add to the rotation. |
| `face.align_uv {faces, action, as_group?}` | `{type:"fit", repeat?:[u,v], axes?}` covers the face `repeat` times (default once; `axes` u, v or both). `{type:"justify", u?, v?}` puts the face's start, centre or end edge on the image's. `{type:"flip", axis}` mirrors the image while the face keeps showing the same part. `{type:"reset", to?}` starts over: `paraxial` (default) or `face_aligned`. |
| `face.transfer_uv {from, to, mode?}` | `copy` copies material and attributes. `wrap` (default) continues the source's image onto the targets: across the edge they share, as if folded flat; with no shared edge it projects along the source's mapping. |
| `face.set_material {faces, material}` | Assign. Warns when the name is not defined. |
| `face.get_uv {faces}` | Read back. |

`as_group: true` treats all listed faces as one surface: the first face's mapping is projected onto the others and the fit or justify uses the bounds of all of them, so one image runs across several faces. For example, `face.align_uv` fit over the tops of two adjacent boxes puts one image across both.

Faces are named like `b12/f3`; a bare brush or mesh id means all its faces.

## Materials ops

`material.list`, `material.get`, `material.create`, `material.set`, `material.rename` (rewrites the faces that use it, in the same step), `material.embed`, `material.delete` (destructive; `replace_with` renames the faces to another material, otherwise they keep the name and draw as a flat colour). A material can be named by name or id.

## Face selection

Face selection is shared session state, separate from the object selection and not saved: `select.faces.set|add|remove|clear|get`. The GUI's Surface tool writes it and an agent can read it to see what the user picked, or set it to point something out. Commands never read it; they take explicit faces. It is pruned when its objects or faces disappear.

## Rendering

`editor-render` builds one triangle batch per material. Each vertex carries a uv; the fragment shader computes `texture × vertex colour (Lambert shading, selection tint) × material colour + emissive`. Images are decoded once and cached by file path and modification time (or by the hash of embedded bytes); GPU textures are cached by the same key and get a CPU-built mip chain. The sampler repeats and, for `linear`, filters anisotropically. Textures are sampled as plain (non-sRGB) values, like every other colour in the viewport.

Screenshots draw images by default; `view.screenshot` takes `textures: false` for flat colours, and `colors` other than `material` (orientation, layer) also draw flat. Selected faces are tinted orange with an outline when the `selection` overlay is on. The legend lists the textured materials separately.

The GUI watches files only by rebuilding when the document changes; after editing an image on disk, touch the scene (any edit) or use "Pick image…" again to refresh.

## GUI

- **Surface tool** (`0`): click selects a face (Shift adds or toggles), Ctrl+click gives the face the material chosen in the inspector, Ctrl+Shift+click wraps the first selected face's image onto the clicked face. With faces selected, arrow keys nudge the offset by the grid size (Shift: 1 texel) and Page Up/Down rotate by 15°.
- **Inspector, faces:** offset, scale and rotation fields (a `≠` marks values that differ between the selected faces; editing changes only that field on all of them), nudge, scale, rotate, flip, reset, fit with repeat counts and "one surface", a justify grid and "Wrap from first".
- **Inspector, materials:** thumbnails (click to choose and assign to the selected faces, or to the selected objects), filter, new, new from image, names faces use that no material defines, and an editor for the chosen material: name, image (pick, store as relative/absolute/embed, embed, clear), tint, emissive, roughness, metallic, filter, flags, delete.

## Future

Not done: per-face tint, lightmaps, material search paths or a materials directory (waits for per-project config), UV editing by dragging in the viewport, a UV-space view, and re-fitting UVs through clip and CSG.
