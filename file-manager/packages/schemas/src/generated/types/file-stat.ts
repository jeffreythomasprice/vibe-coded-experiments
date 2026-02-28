// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate


/**
 * Metadata for a single file or directory (timestamps as ISO 8601 strings)
 */
export interface FileStat {
name: string
path: string
type: ("file" | "directory" | "symlink")
/**
 * Size in bytes
 */
size: number
/**
 * ISO 8601
 */
createdAt: string
/**
 * ISO 8601
 */
modifiedAt: string
mimeType?: string
}
