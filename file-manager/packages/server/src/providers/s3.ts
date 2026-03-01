import {
    S3Client,
    ListObjectsV2Command,
    HeadObjectCommand,
    GetObjectCommand,
    PutObjectCommand,
    DeleteObjectCommand,
    DeleteObjectsCommand,
    CopyObjectCommand,
} from "@aws-sdk/client-s3";
import { Upload } from "@aws-sdk/lib-storage";
import { Readable } from "node:stream";
import type { FileEntry, FileStat, StorageProvider } from "@file-manager/shared";

export interface S3ProviderOptions {
    prefix?: string;
    region?: string;
    accessKeyId?: string;
    secretAccessKey?: string;
}

/** Resolve a user-facing path to a full S3 key, accounting for the prefix. */
export function resolveS3Key(prefix: string, filePath: string): string {
    const normalized = filePath.replace(/^\/+/, "");
    if (!prefix) return normalized;
    const trimmedPrefix = prefix.replace(/\/+$/, "");
    if (!normalized) return trimmedPrefix + "/";
    return `${trimmedPrefix}/${normalized}`;
}

/** Convert an S3 key back to a user-facing relative path. */
export function s3RelativePath(prefix: string, key: string): string {
    if (!prefix) return key;
    const trimmedPrefix = prefix.replace(/\/+$/, "") + "/";
    if (key.startsWith(trimmedPrefix)) {
        return key.slice(trimmedPrefix.length);
    }
    return key;
}

export class S3Provider implements StorageProvider {
    private client: S3Client;
    private bucket: string;
    private prefix: string;

    constructor(bucket: string, options: S3ProviderOptions = {}) {
        this.bucket = bucket;
        this.prefix = (options.prefix ?? "").replace(/\/+$/, "");

        const clientConfig: ConstructorParameters<typeof S3Client>[0] = {};
        if (options.region) {
            clientConfig.region = options.region;
        }
        if (options.accessKeyId && options.secretAccessKey) {
            clientConfig.credentials = {
                accessKeyId: options.accessKeyId,
                secretAccessKey: options.secretAccessKey,
            };
        }
        this.client = new S3Client(clientConfig);
    }

    private key(filePath: string): string {
        return resolveS3Key(this.prefix, filePath);
    }

    private relative(key: string): string {
        return s3RelativePath(this.prefix, key);
    }

    async list(filePath: string): Promise<FileEntry[]> {
        let prefix = this.key(filePath);
        if (prefix && !prefix.endsWith("/")) prefix += "/";

        const dirs: FileEntry[] = [];
        const files: FileEntry[] = [];

        let continuationToken: string | undefined;
        do {
            const resp = await this.client.send(
                new ListObjectsV2Command({
                    Bucket: this.bucket,
                    Prefix: prefix,
                    Delimiter: "/",
                    ContinuationToken: continuationToken,
                }),
            );

            if (resp.CommonPrefixes) {
                for (const cp of resp.CommonPrefixes) {
                    if (!cp.Prefix) continue;
                    const rel = this.relative(cp.Prefix);
                    const name = rel.replace(/\/$/, "").split("/").pop() ?? rel;
                    dirs.push({
                        name,
                        path: filePath ? `${filePath.replace(/\/+$/, "")}/${name}` : name,
                        type: "directory",
                        size: 0,
                        modifiedAt: new Date().toISOString(),
                    });
                }
            }

            if (resp.Contents) {
                for (const obj of resp.Contents) {
                    if (!obj.Key || obj.Key === prefix) continue;
                    const rel = this.relative(obj.Key);
                    const name = rel.split("/").pop() ?? rel;
                    // Skip directory marker objects
                    if (name === "" || obj.Key.endsWith("/")) continue;
                    files.push({
                        name,
                        path: filePath ? `${filePath.replace(/\/+$/, "")}/${name}` : name,
                        type: "file",
                        size: obj.Size ?? 0,
                        modifiedAt: (obj.LastModified ?? new Date()).toISOString(),
                    });
                }
            }

            continuationToken = resp.NextContinuationToken;
        } while (continuationToken);

        dirs.sort((a, b) => a.name.localeCompare(b.name));
        files.sort((a, b) => a.name.localeCompare(b.name));

        return [...dirs, ...files];
    }

    async stat(filePath: string): Promise<FileStat> {
        const key = this.key(filePath);
        const name = filePath.split("/").pop() ?? filePath;

        // Root of the mount always exists as a directory (even for empty buckets)
        const normalized = filePath.replace(/^\/+/, "").replace(/\/+$/, "");
        if (!normalized) {
            return {
                name: name || "/",
                path: filePath,
                type: "directory",
                size: 0,
                createdAt: new Date(),
                modifiedAt: new Date(),
            };
        }

        // Try as file first
        try {
            const resp = await this.client.send(
                new HeadObjectCommand({ Bucket: this.bucket, Key: key }),
            );
            return {
                name,
                path: filePath,
                type: "file",
                size: resp.ContentLength ?? 0,
                createdAt: resp.LastModified ?? new Date(),
                modifiedAt: resp.LastModified ?? new Date(),
                ...(resp.ContentType ? { mimeType: resp.ContentType } : {}),
            };
        } catch {
            // Not a file — check if it's a directory prefix
        }

        // Check for directory (any objects under this prefix)
        const dirPrefix = key.endsWith("/") ? key : key + "/";
        const resp = await this.client.send(
            new ListObjectsV2Command({
                Bucket: this.bucket,
                Prefix: dirPrefix,
                MaxKeys: 1,
            }),
        );

        if (resp.Contents && resp.Contents.length > 0) {
            return {
                name,
                path: filePath,
                type: "directory",
                size: 0,
                createdAt: new Date(),
                modifiedAt: new Date(),
            };
        }

        throw new Error(`Not found: ${filePath}`);
    }

    async *read(filePath: string): AsyncIterable<Buffer> {
        const resp = await this.client.send(
            new GetObjectCommand({ Bucket: this.bucket, Key: this.key(filePath) }),
        );

        if (!resp.Body) {
            throw new Error(`Empty response body for: ${filePath}`);
        }

        // The SDK Body is a Readable web stream — iterate it
        const body = resp.Body as AsyncIterable<Uint8Array>;
        for await (const chunk of body) {
            yield Buffer.from(chunk);
        }
    }

    async write(filePath: string, data: AsyncIterable<Buffer>): Promise<void> {
        const upload = new Upload({
            client: this.client,
            params: {
                Bucket: this.bucket,
                Key: this.key(filePath),
                Body: Readable.from(data),
            },
        });
        await upload.done();
    }

    async delete(filePath: string): Promise<void> {
        const key = this.key(filePath);

        // Try deleting as a single object first
        try {
            await this.client.send(
                new HeadObjectCommand({ Bucket: this.bucket, Key: key }),
            );
            await this.client.send(
                new DeleteObjectCommand({ Bucket: this.bucket, Key: key }),
            );
            return;
        } catch {
            // Not a single object — treat as directory prefix
        }

        // Delete all objects under this prefix (recursive directory delete)
        const prefix = key.endsWith("/") ? key : key + "/";
        let continuationToken: string | undefined;
        do {
            const listResp = await this.client.send(
                new ListObjectsV2Command({
                    Bucket: this.bucket,
                    Prefix: prefix,
                    ContinuationToken: continuationToken,
                }),
            );

            const objects = listResp.Contents;
            if (objects && objects.length > 0) {
                // DeleteObjects supports up to 1000 keys per request
                for (let i = 0; i < objects.length; i += 1000) {
                    const batch = objects.slice(i, i + 1000);
                    await this.client.send(
                        new DeleteObjectsCommand({
                            Bucket: this.bucket,
                            Delete: {
                                Objects: batch
                                    .filter((o) => o.Key !== undefined)
                                    .map((o) => ({ Key: o.Key })),
                                Quiet: true,
                            },
                        }),
                    );
                }
            }

            continuationToken = listResp.NextContinuationToken;
        } while (continuationToken);
    }

    async move(src: string, dest: string): Promise<void> {
        const srcKey = this.key(src);
        const destKey = this.key(dest);

        // Try as single file first
        try {
            await this.client.send(
                new HeadObjectCommand({ Bucket: this.bucket, Key: srcKey }),
            );
            await this.client.send(
                new CopyObjectCommand({
                    Bucket: this.bucket,
                    CopySource: `${this.bucket}/${encodeURIComponent(srcKey)}`,
                    Key: destKey,
                }),
            );
            await this.client.send(
                new DeleteObjectCommand({ Bucket: this.bucket, Key: srcKey }),
            );
            return;
        } catch {
            // Not a single file — treat as directory prefix
        }

        // Move all objects under the source prefix
        const srcPrefix = srcKey.endsWith("/") ? srcKey : srcKey + "/";
        const destPrefix = destKey.endsWith("/") ? destKey : destKey + "/";

        let continuationToken: string | undefined;
        do {
            const listResp = await this.client.send(
                new ListObjectsV2Command({
                    Bucket: this.bucket,
                    Prefix: srcPrefix,
                    ContinuationToken: continuationToken,
                }),
            );

            if (listResp.Contents) {
                for (const obj of listResp.Contents) {
                    if (!obj.Key) continue;
                    const relative = obj.Key.slice(srcPrefix.length);
                    const newKey = destPrefix + relative;
                    await this.client.send(
                        new CopyObjectCommand({
                            Bucket: this.bucket,
                            CopySource: `${this.bucket}/${encodeURIComponent(obj.Key)}`,
                            Key: newKey,
                        }),
                    );
                    await this.client.send(
                        new DeleteObjectCommand({ Bucket: this.bucket, Key: obj.Key }),
                    );
                }
            }

            continuationToken = listResp.NextContinuationToken;
        } while (continuationToken);
    }

    async mkdir(dirPath: string): Promise<void> {
        let key = this.key(dirPath);
        if (!key.endsWith("/")) key += "/";
        await this.client.send(
            new PutObjectCommand({
                Bucket: this.bucket,
                Key: key,
                Body: "",
            }),
        );
    }
}
