shape editor
drag to place geometric primitives
boolean operations to group them: union, intersection, subtraction
shapes have styles: outline color and size, background color, background image

how to handle boolean operations:
- keep original shapes forever, result of bool ops is rendered to polygons only at the last step before aplying styles
- apply boolean ops and keep the resulting geometry around as a new top-level shape
- both?

interface:
- left sidebar is a list of tools:
	- select
		- shift click will add multiple items to selection
	- drag for new rectangle
	- drag for new circle
- right sidebar is the currently selected stuff
	- collapsable for the boolean operations that group these shapes together
		- options for breaking out pieces, e.g. delete a union node splits up to 3 new shapes out: the parent (if any), and the 2 children that were unioned
	- collapsable for material
- when something is selected outline the whole boolean shape, and outline in a different color the individual shapes that make it up

show a grid on the background
grid size is configurable

move selected items by click dragging, or by arrow keys
shift + arrow keys or shift + drag locks to grid

direct polygon editor, like be able to select and move vertices or edges?
apply style to edges individually instead of the shape as a whole?

select should have multiple modes:
- everything
- shapes only
- edges only
- vertices only

if you keep clicking on a point you cycle through all the possible things your current select mode could select there
sort be Z, and then by type?
e.g. in everything mode you cycle through:
- shape/group with highest z
- shape/group with next highest z
- etc.
- edge on shape with highest z
- edge on shape with next highest z
- etc.
- vertex on shape with highest z
- vertex on shape with next highest z
- etc.

some amount of fuzzing, e.g. you select vertices and edges if you are sort of close to them, doesn't need to be dead on

how to do easy mode quick sketches?
e.g. workflow would be to click and drag several times to make some overlapping rectangles
then click a button and "fix" them all so there are nice non-overlapping shapes for the rooms with lines between them
then a another easy way to add "doors" on the lines between rooms
then we change our mind about the size of a room so we drag one edge to resize