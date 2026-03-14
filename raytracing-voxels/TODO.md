I want water physics.

If the player intersects a water block, we need to distinguish between the following states:
- UNDERWATER = eye underwater
- TREADING_WATER = eye y <= 0.5 voxels of water
- ABOVE_WATER = eye y > 0.5 voxels of water

The blue filter applies if in the UNDERWATER state. This should be roughly equivalent to the current logic.

We should be slower in any of these water states.

Holding space while in UNDERWATER or TREADING_WATER should move the camera up at a constant slow rate.

Holding space while in ABOVE_WATER should do nothing, and the player should fall back again.



hung, unkillable, 100% cpu

shadows for point light sources

wibbly shader that makes water move?

clouds

biomes
mountains
plains
ocean
plains with lakes?
forest
