-- A sample map so the client renders something out of the box before any
-- group-builder UI exists. The group is `A UNION (B SUBTRACT C)`: a base
-- rectangle unioned with a second rectangle that has a square hole punched out.
-- Plus one standalone rectangle. The `content` column is JSON matching the
-- serde representation of {groups, shapes}.
INSERT INTO maps (id, name, width, height, grid_size, grid_unit, background_color, grid_color, content, updated_at)
VALUES (
    'seed-map',
    'Sample Dungeon',
    20,
    15,
    5.0,
    'ft',
    '#1e1e28',
    '#3a3a4a',
    '{"groups":[{"id":"seed-group","style":{"line_color":"#ffd24a","line_width":2.0,"background_color":"#6a4a2a"},"root":{"node":"op","op":"union","left":{"node":"leaf","shape":{"id":"seed-a","geometry":{"shape":"rect","x":2,"y":2,"w":6,"h":4},"style":{"line_color":"#ffffff","line_width":1.0,"background_color":"#888888"}}},"right":{"node":"op","op":"subtract","left":{"node":"leaf","shape":{"id":"seed-b","geometry":{"shape":"rect","x":6,"y":4,"w":6,"h":5},"style":{"line_color":"#ffffff","line_width":1.0,"background_color":"#888888"}}},"right":{"node":"leaf","shape":{"id":"seed-c","geometry":{"shape":"rect","x":8,"y":6,"w":2,"h":2},"style":{"line_color":"#ffffff","line_width":1.0,"background_color":"#888888"}}}}}}],"shapes":[{"id":"seed-standalone","geometry":{"shape":"rect","x":1,"y":11,"w":4,"h":3},"style":{"line_color":"#4ad2ff","line_width":2.0,"background_color":"#2a4a6a"}}]}',
    strftime('%s', 'now')
);
