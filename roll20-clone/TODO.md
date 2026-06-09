I want to have the concept of a map.

`shared` should have data structures and functions for maps:
- shape = geometric primitive
- group = collection of shapes grouped together via boolean operators: e.g. `shape A` UNION (`shape B` SUBTRACT `shape C`)
- we support UNION, INTERSECT, and SUBTRACT operators
- groups have properties like line color, line width, and background color
- map = a rectangular area defined with a width and height (in grid units), and a grid size (in real world units, e.g. 1 square = 5 ft)
- maps also have a background color and a grid color
- maps have a list of groups and shapes (i.e. shapes not in groups)

`server` needs a set of APIs and db work to manage maps. We should be able to do CRUD on maps, and given a map on the shapes and groups inside it. We should have an API where clients register with the server and request a particular map to follow. A client can only follow one map, and trying to follow a second succeeds by following that instead of the first. When updates are made to a map that should be sent via websockets to all connected clients that are following that map.

`client` should display a UI that lists all the maps and some basic information about them. Selecting a map moves the UI to a map view that uses canvas to render all the the groups and shapes. The canvas should be scrollable via middle mouse drag or arrow keys. The mouse wheel or the +- keys should zoom. We should have a max and min zoon, and restrict the pan region such that at least some of the map is always visible.

The client should have a sidebar on the left that lets you pick different tools. The tool list for right now should be:
- select tool
- create new rectangle tool

When the select tool is selected we should be able to click and select whatever group or shape is under the mouse cursor. Shift clicking should add more items to the selection.

When the create new rectangle tool is selected we should be able to click and drag to create a new shape.

We should have a sidebar on the right that displays information about the currently selected shapes or groups. This can be a list of information about those items. When we're displaying a group we should display the tree of boolean operations with the individual shape details as leaf-level items. We should be able to modify properties of shapes and groups that are selected. When all the selected items have the same value (e.g. the same line width or background color) we can display it. If we've selected multiple things with different values we can display a warning (e.g. "multiple selected") but still allow the user to change all of them at once (e.g. to modify several objects with different properties to be the same background color).




hex grids

logins

miniatures / player figures on the map

ruler tool

graphical effects like explosions

textures

chat