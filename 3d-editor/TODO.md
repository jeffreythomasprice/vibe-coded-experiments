in flight:

todo:

just keep chewing through plan.md


Run an mcp test. Make sure to output screenshots to a directory and give me the path so I can review. I also want you to review the screenshots yourself in subagents. Make sure to give the subagents a carefully controlled context so they don't assume things about the images but give a summary of just what they see. Summarize the results for me so I can compare against the images myself.


Make sure to update CLAUDE.md, README.md, and docs. It's ok to create new files in docs as appropriate, e.g. if we've built new big architectural elements.


Propose next steps based on these results and plan.md. Include a handoff prompt I can give to a new agent that will get started.


Make the trial repeatable
- Turn the build into a level-cli session JSONL script plus a fixture level. Then a rerun (and the blind-reviewer step) can be repeated after each surface change instead of rebuilt by hand.
- Exercise the ops not yet covered: select_*, history_undo/history_redo, and the face and mesh-conversion ops.



● Proposed next steps

  The agent path (steps 1–4 of plan.md §8) is now solid. The blind reviews found the remaining gaps in how images read, not in the ops. Plan risk 6, agent usability, says to iterate on the
  tool surface with real trials, so I'd do one more small legibility round and then move on to step 5, the egui shell.

  1. Renderer legibility (small, do first)

  Each item maps to a reviewer finding:

  1. Section views look hollow (images 02 and 05). clip_above draws no cap, so cut solids show their back faces as "faces down (ceilings)".
     - Cheapest fix: draw back faces in a distinct flat "cut / inside of solid" colour and add a legend entry. It needs no stencil pass.
     - Stencil capping is the proper fix and can wait.
  2. Slopes and surfaces don't read in shaded mode (image 05).
     - The ramp looked like a flat slab, and one brick material turned the whole vault the same blue.
     - Check the current lighting in level-render/src/scene.rs and the shader. Increase normal-based contrast so floors, ceilings, walls and slopes differ even under one material.
     - Re-shoot image 05 as the pass/fail check.
  3. Labels cover small objects (images 02 and 03).
     - For objects whose visible area is smaller than the label box, always place the label beside the object with a leader line.
     - The b15 anchor sat on a boundary. Make sure x, y is truly interior.
  4. Void versus ceiling (image 04). Add an optional background colour argument so an agent can pick a contrasting clear colour. This is cheaper than the gradient I proposed and rejected
     earlier.
  5. Entities (images 02 and 04). The player-start marker was unlabelled and unidentified. Label entity markers with their classname, or at least make the legend say what each marker is
     where it is drawn.

  2. API consistency (also small)

  - object.duplicate returns map as old -> new. object.array, brush.clip and csg.subtract return old -> [new]. Decide whether to unify this and document it in tier0-commands.md.
  - plan.md step 4 notes "args schemas only; result schemas … come later". Result schemas would help agents. Consider adding them for the ops whose results agents read most: scene.*,
    room.*, view.screenshot.

  3. Run another agent trial (risk 6)

  Give a fresh agent a different level brief over MCP, with no design hints. Measure calls and failures, and have blind reviewers check the screenshots as I did here. Decide on the arch op
  and the unbuilt hide_ceilings only if the trial shows they are needed.

  4. Then plan step 5: the egui shell

  editor-app is still a stub. Start with the risk 2 spike: egui_dock with four live viewports on the shared wgpu device, using the existing offscreen renderer. That also covers risks 3 and
  4: version pinning and per-pane input routing.

  Housekeeping

  - There is a large uncommitted diff from both sessions. Review and commit it yourself, since I can't run state-changing git commands here.
  - CI needs lavapipe (mesa-vulkan-drivers) for the render tests (risk 5).

  Handoff prompt (for items 1 and 2)

  You're working in the Rust workspace at /home/jeff/workspaces/personal/vibe-coded-experiments/3d-editor (a TrenchBroom-style brush level editor driven by humans and AI agents over
  MCP/CLI).

  Read CLAUDE.md first and follow it strictly: no git commands that change state (never commit), never read or edit TODO.md, minimal comments, specific error enums (no anyhow), keep
  README.md terse, update docs/ when a design decision changes. Then read docs/plan.md (risk 6, section 8) and docs/tier0-commands.md (the view.screenshot section especially).
  level_cmd::OPS in crates/level-cmd/src/commands/mod.rs is the single source of truth for ops; level-cmd must stay GPU-free; view.screenshot is an Exec::Client op that level-host routes to
  level_render.

  Background: an agent rebuilt a hall + corridor + vault test level over MCP, then six reviewers who were told nothing about the design each described one screenshot. Everything measurable
  was read correctly (pillar counts, stair count, flush connector, heights). What read badly:
   A. Section views (view.screenshot clip_above) draw no cap on cut solids, so cut walls and pillars show their back faces; in colors:"orientation" they appear in the "faces down
  (ceilings)" colour.
   B. A wedge ramp (brush.create_wedge) looked like a flat slab, and a room built from one material (all brick) had indistinguishable floor, ceiling and walls in shaded mode.
   C. Labels cover small objects (25px pillars, thin wall strips); one label (b15) sat on a boundary and read as belonging to nothing; leader lines are tiny.
   D. With a ceiling hidden, the dark clear colour reads as "void or sky" and cannot be told apart.
   E. Entity markers (player start, lights) are unlabelled and unexplained in the image.
  Screenshots and the level are in /tmp/claude-1000/-home-jeff-workspaces-personal-vibe-coded-experiments-3d-editor/05e3be23-c6a3-4390-a1b5-ac6de7e69e9f/scratchpad/mcp-run/ (shots/*.png and
  world.level.toml; if missing, rebuild a similar level with room.create x3, room.cut_opening, brush.create_box + object.array with mirror, brush.create_wedge, stairs.create). Read the
  PNGs yourself.

  Do, in order, each finished with tests and docs:
  1. Fix A: draw back faces of clipped solids in a distinct flat "cut (inside of solid)" colour, with a legend entry; no stencil pass.
  2. Fix B: read the lighting in crates/level-render/src/scene.rs and shader.wgsl and increase normal-based contrast so floors, ceilings, walls and slopes differ under one material. Keep
  flat and wireframe modes unchanged.
  3. Fix C: for objects whose visible area is smaller than the label box, place the label beside the object with a clearly visible leader line; make sure label x,y is inside the object's
  visible pixels.
  4. Fix D: add an optional `background` ("#rrggbb") argument to view.screenshot, reported in legend.background.
  5. Fix E: label entity markers (or list where each marker is drawn) so an image identifies the player start and lights.
  6. Unify or explicitly document the `result.map` shape difference (object.duplicate maps old -> new string; array, clip and csg map old -> [new]) in docs/tier0-commands.md.
  Render tests need a wgpu adapter: run them with `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p level-render`.

  Verify with cargo test --workspace, cargo clippy --workspace --all-targets, and the render tests. Then rebuild the test level through the `editor` MCP tools (restart Claude Code after
  cargo build if the tool list is stale), save screenshots to a new directory, and have fresh subagents each describe one image, giving them only the image path and a neutral describe-only
  brief (no design info, no hypotheses). Report what changed, what you declined to do and why, and new problems. Do not commit.
