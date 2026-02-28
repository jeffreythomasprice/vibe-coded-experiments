// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate


/**
 * Provider mount registration
 */
export interface MountInfo {
mountId: string
scheme: ("local" | "s3" | "gcs" | "sftp" | "smb")
config: {
[k: string]: string
}
}
