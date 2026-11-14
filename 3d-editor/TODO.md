in flight:



todo:

I want to do this todo item:
- [ ] Agent usability trials (§7.6). Run real agent sessions against the MCP tools and tune how fine-grained or coarse the tools are.

I'm looking to have an agent produce a non-trivial scene and then evaluate how well it did. We can have the agent record a log of pain points as it goes. Maybe even journal it's feedback on the process as we go. I want to run another subagent at the end that gets as input the final scene and the original prompt and evaluates it for completes. A separate subagent can get the agent's pain points log and suggest some improvements. Summarize the final results for me, and save the final scene and log somewhere I can review them manually. The goal is run this procedure, make changes based on that feedback, then run it again and compare results.

Suggestion for a big test scene:
- an office building with multiple floors
- cubicals with geometric shapes for things like desks, chairs, computers
- offices along the wall with doors and windows
- the doors have moving keyframes
- a stairwell between floors
- an elevator between flors, with moving keyframe



just keep chewing through plan.md


Run an mcp test. Make sure to output screenshots to a directory and give me the path so I can review. I also want you to review the screenshots yourself in subagents. Make sure to give the subagents a carefully controlled context so they don't assume things about the images but give a summary of just what they see. Summarize the results for me so I can compare against the images myself.


Propose next steps based on current state and plan.md. Include a handoff prompt I can give to a new agent that will get started.


editor support for some entities: keyframe movement, buttons, zones, lights, sounds
this might just be a json schema to form automation so we can pretty-print the form for the schema


game mode should have occlusion culling or octrees or something


lighting


D. Tier 2: productivity

Already built early: room.create, room.cut_opening, stairs.create, check.validate.

Still to do:
- [ ] More shape tools: arch, cylinder, cone, spheroid, and a convex hull from clicked points.
- [ ] Extrude (with split mode) and sweep.
- [ ] UV editor: drag, auto-fit, align, UV lock, auto-align.
- [ ] Linked groups.
- [ ] Command repetition.
- [ ] A live issue browser with quick fixes (the checker itself exists).
- [ ] Rebindable shortcuts, mouse mapping, tablet mode.
- [ ] Entity model display and icons/gizmos.
- [ ] Crash recovery.
- [ ] Agent extras: change subscriptions, named bookmarks, "explain this object", prefab/library queries.

E. Tier 3 and 4

- [ ] Export pipeline and compile profiles, Quake .map import/export, glTF export, per-project config.
- [ ] Tags for special brush/face types, map bounds checks, and preferences, updates and localization.
- [ ] Tier 4 stretch items: patches/terrain, lighting preview, play-in-editor, plugin API, collaboration, extra map formats.

F. Risks and open questions

- [ ] No-GPU CI. Install lavapipe (mesa-vulkan-drivers) on the runner and add a CI job. The GL fallback fails here.
- [ ] Exact predicates for mesh CSG, only if the 0.02% failure rate proves too high.
- [ ] Single file vs. directory format. This stays open, and the parent-pointer schema keeps the split cheap.
- [ ] Docs. Update plan.md status once the GUI work starts. The §3 "(verify)" items (winit lag, egui_dock, theming crates, "egui_styled") are still unresolved.
