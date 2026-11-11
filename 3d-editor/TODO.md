in flight:

My todo list has:
- [ ] Entity definitions schema (our own format, with optional FGD import), an entity browser, and drag-and-drop.
- [ ] Entity link lines (target references) in the viewport.

I'd like to generally make an entity system that works. Some use cases I have in mind that I want to support in a future game or simulation based on this:
- entities that have keyframe animation, like doors or elevators
- entities that trigger other events in game logic, like buttons; e.g. to trigger those doors or elevators, or to do some other scripting event
- entities that represent invisible zones, e.g. to track player movement in a region
- point entities, like lights or sound sources

Our current model assumes entities are a class name plus a set of key-value properties. I'd like to expand the key-value properties to a json schema. For now we don't need to build a full UI form for it, but just present a text area that will do json schema validation against the input. We'll do a nice form auto generated from the scema in a later step. For right now the editor doesn't need to really understand most of the class names or their associated data, this is just for future proofing us for complicated ECS stuff later.

The one exception for this data is that we need a hook in the json schema that represents an entity ID. I'm imagining something like:
- two entities, named "A" and "B"
- entity A uses a schema like:
{
  "type": "object",
  "properties": {
    "link": {
      "type": "string",
      "editor": {
        "linkType": "entityID"
      }
    }
  }
}


todo:

just keep chewing through plan.md


Run an mcp test. Make sure to output screenshots to a directory and give me the path so I can review. I also want you to review the screenshots yourself in subagents. Make sure to give the subagents a carefully controlled context so they don't assume things about the images but give a summary of just what they see. Summarize the results for me so I can compare against the images myself.


Propose next steps based on these results and plan.md. Include a handoff prompt I can give to a new agent that will get started.


editor support for some entities: keyframe movement, buttons, zones, lights, sounds


C. Tier 1: brush editor

Still to do:
- [ ] Layer hide/lock and omit-from-export, group open/close with lock, and hide/isolate/lock/show-all. All undoable. I found no visibility or lock state in the model.
- [ ] Clipboard: copy/paste in place or at cursor.
- [ ] Autosave and backups.

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
- [ ] Agent usability trials (§7.6). Run real agent sessions against the MCP tools and tune how fine-grained or coarse the tools are.
- [ ] Exact predicates for mesh CSG, only if the 0.02% failure rate proves too high.
- [ ] Single file vs. directory format. This stays open, and the parent-pointer schema keeps the split cheap.
- [ ] Docs. Update plan.md status once the GUI work starts. The §3 "(verify)" items (winit lag, egui_dock, theming crates, "egui_styled") are still unresolved.
