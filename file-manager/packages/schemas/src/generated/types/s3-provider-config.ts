// GENERATED FILE — DO NOT EDIT MANUALLY
// Run `bun run --cwd packages/schemas generate` to regenerate


/**
 * Configuration for the S3 storage provider
 */
export interface S3ProviderConfig {
/**
 * S3 bucket name
 */
bucket: string
/**
 * Key prefix (virtual directory) within the bucket
 */
prefix?: string
/**
 * AWS region (e.g. us-east-1)
 */
region?: string
/**
 * AWS access key ID (omit to use default credential chain)
 */
accessKeyId?: string
/**
 * AWS secret access key (omit to use default credential chain)
 */
secretAccessKey?: string
}
