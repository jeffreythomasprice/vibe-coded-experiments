// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate


/**
 * A single entry in a file listing (JSON-serializable form)
 */
export interface FileEntry {
/**
 * Filename or directory name (basename)
 */
name: string
/**
 * Full path within the mount
 */
path: string
/**
 * Entry type
 */
type: ("file" | "directory" | "symlink")
/**
 * Size in bytes
 */
size: number
/**
 * Last-modified timestamp (ISO 8601)
 */
modifiedAt: string
}
