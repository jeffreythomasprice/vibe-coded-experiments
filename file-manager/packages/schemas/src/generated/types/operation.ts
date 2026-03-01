// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate


/**
 * Tracks progress of a file operation (copy, move, upload, or delete)
 */
export interface Operation {
id: string
type: ("copy" | "move" | "upload" | "delete")
status: ("pending" | "in_progress" | "completed" | "failed")
src: string
dest?: string
totalBytes: number
processedBytes: number
totalFiles: number
processedFiles: number
currentFile?: string
error?: string
/**
 * ISO 8601
 */
createdAt: string
/**
 * ISO 8601
 */
updatedAt: string
}
