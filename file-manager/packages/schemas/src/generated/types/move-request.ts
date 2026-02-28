// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate


/**
 * File move/rename operation
 */
export interface MoveRequest {
/**
 * Source URI: <scheme>://<mountId>/<path>
 */
src: string
/**
 * Destination URI
 */
dest: string
}
