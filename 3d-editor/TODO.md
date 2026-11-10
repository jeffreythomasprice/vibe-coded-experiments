in flight:

todo:

● Handoff: level-mcp surface fixes after the agent trial

  Shared context

  This is a brush level editor for humans (GUI) and AI agents (MCP/CLI), in a Cargo workspace under crates/. Read CLAUDE.md first, plus docs/plan.md and docs/tier0-commands.md. The GUI
  (editor-app) is still an empty stub.

  Repo rules from CLAUDE.md
  - No git commands that change state.
  - Don't read or edit TODO.md.
  - Keep README.md terse.
  - Avoid comments.
  - Use strict error enums, not anyhow.
  - Update docs/ when a design decision changes.
  - level_cmd::OPS (the ops! table in crates/level-cmd/src/commands/mod.rs) is the single source of truth. MCP tools and the CLI ops/schema commands are generated from it.
  - level-cmd must stay GPU-free. view.screenshot is an Exec::Client op that level-host routes to level_render.

  Checks: cargo test --workspace, cargo clippy --workspace --all-targets. Render tests need a wgpu adapter. Check headless with env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p level-render.

  Where the trial came from. I built a three-room level (hall, rotunda, gallery) through the MCP tools only, to test plan risk 6 (agent usability). Artifacts:
  - /tmp/editor-test/world.level.toml (the level)
  - /tmp/editor-test/shots/ (14 PNGs)
  - /tmp/editor-test/trial-notes.md (the full friction log)

  /tmp may be gone, so the findings are restated below. The trial took about 25 tool calls for three rooms.

  Uncommitted state from the trial session
  - file.save now takes overwrite and file.new takes path.
  - There is a new CommandError::FileExists.
  - .mcp.json and .claude/settings.local.json are untracked local config. Don't commit them.
  - To reproduce the trial: run cargo build -p level-mcp -p level-cli, restart Claude Code, and the level MCP server connects (started with --allow-destructive, no file argument).

  Do the work in the order 1, 2, 3. Item 3 depends on 1. Item 2 is independent of both, so it can run in parallel in a separate worktree.

  ---

  Item 1: batch and result-shape fixes

  Prompt to paste

  You're working in the Rust workspace at /home/jeff/workspaces/personal/vibe-coded-experiments/3d-editor (a brush level editor driven by humans and AI agents over MCP/CLI). Read CLAUDE.md,
  docs/tier0-commands.md and docs/tier0-mcp.md first and follow CLAUDE.md strictly (no state-changing git commands, don't touch TODO.md, minimal comments, specific error enums).

  Background: an AI agent built a test level through the MCP tools and hit friction that the command surface should fix before a GUI locks it in. Your job is the following list; ops live in
  the `ops!` table in crates/level-cmd/src/commands/mod.rs, session logic in crates/level-cmd/src/session.rs, errors in crates/level-cmd/src/error.rs, and the MCP server in
  crates/level-mcp/src. Find where `batch` and its `$N` / `$N.ids[k]` substitution are implemented before changing anything.

  1. batch op names: inside `batch.commands`, accept both dotted (`brush.create_box`) and underscored (`brush_create_box`) names. The MCP tool names are underscored but batch currently only
  accepts dotted, and the agent guessed wrong.
  2. Named batch refs: let a batch item carry an optional `as` name so later items can reference `"$name"` / `"$name.ids[k]"`. `$N` is a command index, and an agent deleted the wrong wall
  by pointing at the wrong index. Keep the numeric form working. Reject duplicate names with a clear error.
  3. Composite refs: `"$N/v2"` and `"$N/f1"` (a vertex/face of an object created earlier in the batch) are not expanded and fail with "unknown id prefix" and no hint. Make them work, e.g.
  mesh.create followed by vertex.translate on the new mesh in one batch.
  4. Consistent change reporting: brush.clip edits brushes in place but returns `ids: []` and an empty `changes.modified`, so the caller can't tell what happened. csg.subtract and
  brush.clip should report created/modified/deleted ids the same way every other op does. Audit other ops for the same gap.
  5. csg.subtract `consume_cutters` (bool, default false): when true, delete the cutters in the same step. Today every doorway needs csg.subtract plus a separate object.delete.
  6. Error hints: deserialization errors (e.g. mesh.create faces are `{verts, material}` objects, not arrays; the agent got field "0" with an empty hint) should name the expected shape and
  give a short example in `hint`. A generic fix in the arg-parsing path is better than per-op strings.
  7. check.validate messages for mesh_open (and other mesh issues) print Rust Debug text like `OpenEdge { a: VertexId(1), b: VertexId(2) }`. Report them in the ids the tools accept
  (`m84/v1`) and fill `ids` accordingly.
  8. Revision inside transactions: after history.begin, edits report the same revision, so expect_revision can't be used mid-transaction. Decide and document the intended behaviour (propose
  it in docs/tier0-commands.md before coding if it changes semantics; the simplest option may be to keep it and document it).
  9. file.save to a new path on a fresh document leaves the document name as "untitled"; derive it from the file stem like file.new {path} does (see session.rs stem_name).

  For each: add tests next to the existing ones (crates/level-cmd/tests/), update docs/tier0-commands.md (and tier0-mcp.md where MCP-visible), and keep OPS summaries accurate. Finish with
  cargo test --workspace and cargo clippy --workspace --all-targets, then report what changed and anything you decided not to do and why. Do not commit.

  ---

  Item 2: screenshot gaps (occlusion-aware labels, hide/clip, colours, saving)

  Prompt to paste

  You're working in the Rust workspace at /home/jeff/workspaces/personal/vibe-coded-experiments/3d-editor (a brush level editor driven by humans and AI agents over MCP/CLI). Read CLAUDE.md,
  docs/plan.md (section 8, step 3), docs/tier0-commands.md (view.* section) and docs/tier0-mcp.md first and follow CLAUDE.md strictly. Architecture constraints: crates/level-cmd must stay
  GPU-free; `view.screenshot` is an Exec::Client op that crates/level-host routes to level_render::screenshot_with_selection; rendering lives in crates/level-render and reads the document
  through level-geom. Tests that need a GPU: `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p level-render`.

  Background: an AI agent built a three-room level through MCP and a reviewer looked at the PNGs. The vision tooling was the weakest part. Fix these:

  1. Occlusion-aware labels (the one gap plan step 3 lists as not done). The `labels` overlay currently lists every brush/mesh/point entity whose bounds centre is in frame, including things
  behind walls: an eye-level hall view had ~80 labels piled on the far doorway. Make a label appear only when its object is actually visible (e.g. depth-test or raycast the label anchor,
  or use a per-object visible-pixel count from an id buffer) and preferably anchor it to a visible point rather than the bounds centre. Add decluttering (skip or offset labels that would
  overlap, with a cap) and a way to restrict labels to given ids/layers. The result's `labels` array should only contain what is drawn; consider a `visible_fraction` field. Update the docs
  note "A label does not mean the object is unoccluded".
  2. Hiding and sectioning: top-down ortho only sees ceilings. Add `hide` (object ids and/or layers) to view.screenshot args, and a clip plane or height limit (e.g. `clip_above: z`) so plan
  and section views are possible without editing the document. These must not mutate the document. Respect the args-schema/OPS conventions so MCP and `level-cli screenshot` pick them up
  (the CLI needs flags too).
  3. Colour legibility: colours are hashed from material names, so walls/floors/ceilings often share hues and an all-grey room was unreadable. Add an optional `colors` mode or per-material
  override argument, and a legend (material -> colour) in the result so an agent can interpret the image. Give point entities a distinct marker per classname class (light, player start,
  others) or at least vary colour.
  4. Saving: MCP view_screenshot returns the image inline only. Add an optional `path` to the MCP-facing call so the server writes the PNG too (level-cli session mode already has `path`).
  Check how the host policy should treat writing a file (it's a side effect; decide whether it needs mutate access and document it).
  5. Camera framing: default perspective viewport framing is poor for wide levels. Check view.frame and the session cameras' framing for scenes much wider than tall.

  Write render tests where feasible (they need an adapter), docs updates in tier0-commands.md/tier0-mcp.md, and report anything you chose not to do. Reproduction level: if
  /tmp/editor-test/world.level.toml still exists, use `target/debug/level-cli /tmp/editor-test/world.level.toml screenshot --camera hall_eye -o /tmp/x.png --overlay labels` (cameras:
  overview, hall_eye, rotunda_eye, gallery_eye, plan, elevation) to see the label clutter before and after; otherwise build a small multi-room level with level-cli. Finish with cargo test
  --workspace and cargo clippy --workspace --all-targets. Do not commit.

  ---

  Item 3: convenience ops plus a second trial

  Prompt to paste

  You're working in the Rust workspace at /home/jeff/workspaces/personal/vibe-coded-experiments/3d-editor (a brush level editor driven by humans and AI agents over MCP/CLI). Read CLAUDE.md,
  docs/plan.md (risk 6, Tier 2 shape tools), docs/tier0-commands.md and docs/tier0-mcp.md first and follow CLAUDE.md strictly. New ops are added as rows in the `ops!` table in
  crates/level-cmd/src/commands/mod.rs; MCP tools and CLI ops/schema are generated from it. This task assumes the batch/result-shape fixes (named batch refs, consume_cutters on
  csg.subtract, consistent created/modified ids from clip/csg) are already done; if they are not, stop and say so.

  Background: an AI agent needed about 25 tool calls to build three rooms. The cost was mostly boilerplate: a room is 6 box brushes, each doorway is create-cutter + csg.subtract +
  delete-cutter, a staircase is N overlapping boxes. Add coarse ops as thin layers over existing commands (they must be single undoable steps, transactional, with dry_run and structured
  errors like everything else):

  - room.create {min, max, wall (thickness), material?, wall_material?/floor_material?/ceiling_material?, layer?, group?, open_sides?}: a hollow room as floor, ceiling and 4 wall brushes
  with interior dimensions min..max (decide and document whether min/max are interior or exterior); returns ids by role (floor, ceiling, wall_n/s/e/w).
  - brush.cut_doorway or room.cut_opening {target walls, min, max (or centre/width/height on a given wall)}: cut an opening through the wall brushes it intersects, consuming the cutter,
  returning the replacement ids. Think about corridors between rooms.
  - stairs.create {origin, direction (+x/-x/+y/-y), steps, rise, run, width, material?}: N step brushes (non-overlapping preferred, each step solid from the floor up, or stacked as the
  existing convention suggests), returning ids in order.
  Optionally propose (not necessarily build) arch, ramp and pillar-row ops if the design shows they'd save the most calls.

  Design first: write the proposed args/results in docs/tier0-commands.md and sanity check that they compose with groups/layers/entity brushes. Then implement with tests in
  crates/level-cmd/tests/, using the geometry in level-geom, not new maths in level-cmd.

  Then re-run the trial to measure: register the MCP server (a local .mcp.json and .claude/settings.local.json exist for `level`, pointing at target/debug/level-mcp with --allow-destructive
  and no file arg; run `cargo build -p level-mcp -p level-cli` and restart Claude Code to load it). Using only mcp__level__* tools, rebuild the same level: a 512x256x192 entry hall with 6
  pillars (2x3) and a 4-step dais, doorway east into a corridor into an octagonal rotunda (central column, ramp on the north wall), then a corridor to a 512-long gallery with 4 buttresses
  per long wall (mirrored), a chamfered mesh trim strip high on the south wall, and two braziers on rotated square plinths (one scaled), plus lights, a player start, a func_door and
  layers/groups. Bind the file at the start: file_info, then file_save {path: "/tmp/editor-test2/world.level.toml"}. Render screenshots with `target/debug/level-cli <file> screenshot
  --camera <name> -o <png>`. Compare tool-call count, wasted turns and error recoveries with the baseline (~25 calls; baseline notes in /tmp/editor-test/trial-notes.md if it still exists),
  and report which new ops were used, which were missing, and what you'd change. Finish with cargo test --workspace and cargo clippy --workspace --all-targets. Do not commit.

  ---

  Notes for whoever picks these up

  - Baseline pain points, in rough order of turns wasted: wrong $N index (deleted a wall), the dotted-vs-underscored name mismatch, no $N/v2 expansion, cutters never consumed, a mesh.create
    shape error with no hint.
  - Open design question for item 1: whether the revision should advance inside transactions. It is the only item that changes semantics, so settle it in the docs before coding.
  - Open design question for item 2: whether a screenshot path is a side effect that needs mutate access.
  - Verification: all three items finish with cargo test --workspace and cargo clippy --workspace --all-targets. Item 3 also reruns the trial and reports call counts against the roughly
    25-call baseline.
  - Untried ops: brush_create_from_planes, brush_to_mesh/mesh_to_brush, face_set_material/face_set_uv, face_translate, entity_release_objects, select_*,
    scene_raycast/scene_query_box/scene_list, view_frame, dry_run, expect_revision and history_redo/history_list were not exercised in the trial. The item 3 rerun should include them where
    natural.