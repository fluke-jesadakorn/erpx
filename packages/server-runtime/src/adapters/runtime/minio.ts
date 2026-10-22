import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdtemp, open, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { DeleteObjectCommand, GetObjectCommand, HeadObjectCommand, ListObjectsV2Command, PutObjectCommand, S3Client } from '@aws-sdk/client-s3';
import type { FileMetadata, FileStore, SqlDatabase, StoredFile } from "@erpx/server-runtime/adapters/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";

const validKey = (key: string) => {
  if (!key || key.includes('\0') || key.split(/[\\/]/).some(part => part === "." || part === "..")) throw new Error('INVALID_OBJECT_KEY');
  return key;
};
const missing = (error: unknown) => (error as { name?: string }).name === 'NotFound' || (error as { name?: string }).name === 'NoSuchKey';
type UploadSource = { body: Buffer | ReturnType<typeof createReadStream>; size: number; checksum: string; cleanup: () => Promise<void> };
const uploadSource = async (value: Parameters<FileStore['put']>[1]): Promise<UploadSource> => {
  if (!(value instanceof Blob) && !(value instanceof ReadableStream)) {
    const body = typeof value === 'string' ? Buffer.from(value) : ArrayBuffer.isView(value)
      ? Buffer.from(value.buffer, value.byteOffset, value.byteLength) : Buffer.from(value);
    return { body, size: body.length, checksum: createHash('sha256').update(body).digest('hex'), cleanup: async () => {} };
  }
  // S3 requires a known content length. Spool streams to disk so large uploads do
  // not occupy the server heap, while computing the checksum in the same pass.
  const directory = await mkdtemp(join(tmpdir(), 'erpx-minio-'));
  const path = join(directory, 'body');
  try {
    const file = await open(path, 'wx', 0o600);
    const hash = createHash('sha256');
    let size = 0;
    try {
      const source = value instanceof Blob ? value.stream() : value;
      for await (const chunk of Readable.fromWeb(source as never)) {
        const bytes = Buffer.from(chunk);
        hash.update(bytes);
        size += bytes.length;
        let offset = 0;
        while (offset < bytes.length) offset += (await file.write(bytes, offset, bytes.length - offset)).bytesWritten;
      }
    } finally { await file.close(); }
    return { body: createReadStream(path), size, checksum: hash.digest('hex'), cleanup: () => rm(directory, { recursive: true, force: true }) };
  } catch (error) { await rm(directory, { recursive: true, force: true }); throw error; }
};

export class MinioFileStore implements FileStore {
  private readonly client: S3Client;
  constructor(private readonly bucket: string, private readonly prefix: string, endpoint: string, accessKeyId: string, secretAccessKey: string, region: string, private readonly preservationDb?: PostgresDatabase, private readonly legacy?: FileStore) {
    this.client = new S3Client({ endpoint, region, forcePathStyle: true, credentials: { accessKeyId, secretAccessKey } });
  }
  get supportsTransactionDeletion() { return Boolean(this.preservationDb); }
  private objectKey(key: string) { return `${this.prefix}/${createHash('sha256').update(validKey(key)).digest('hex')}`; }
  private metadata(key: string, head: { ContentLength?: number; LastModified?: Date; Metadata?: Record<string, string> }): FileMetadata {
    const meta = head.Metadata ?? {};
    if (meta['erpx-key'] !== Buffer.from(key).toString('base64url')) throw new Error('INVALID_OBJECT_MANIFEST');
    const etag = meta['erpx-sha256'];
    if (!etag || !/^[a-f0-9]{64}$/.test(etag)) throw new Error('INVALID_OBJECT_MANIFEST');
    return { key, size: head.ContentLength ?? 0, etag, httpEtag: `"${etag}"`, uploaded: head.LastModified ?? new Date(), httpMetadata: JSON.parse(meta['erpx-http'] ?? '{}'), customMetadata: JSON.parse(meta['erpx-custom'] ?? '{}') };
  }
  async head(key: string) {
    const remote = await this.remoteHead(key);
    return remote ? this.metadata(key, remote) : this.legacy?.head(key) ?? null;
  }
  async get(key: string, options: Parameters<FileStore['get']>[1] = {}): Promise<StoredFile | null> {
    const remote = await this.remoteHead(key);
    if (!remote) return this.legacy?.get(key, options) ?? null;
    const head = this.metadata(key, remote);
    let offset = 0, length = head.size;
    if (options.range) {
      offset = options.range.suffix === undefined ? options.range.offset ?? 0 : Math.max(0, head.size - options.range.suffix);
      length = Math.min(options.range.length ?? head.size - offset, head.size - offset);
      if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 1 || offset >= head.size) throw new RangeError('INVALID_RANGE');
    }
    const range = options.range ? `bytes=${offset}-${offset + length - 1}` : undefined;
    const read = async () => {
      const result = await this.client.send(new GetObjectCommand({ Bucket: this.bucket, Key: this.objectKey(key), Range: range, IfMatch: remote.ETag }));
      const body = result.Body?.transformToWebStream() as ReadableStream<Uint8Array> | undefined;
      if (!body) throw new Error('OBJECT_BODY_MISSING');
      return body;
    };
    const body = await read();
    return { ...head, ...(options.range ? { range: { offset, length } } : {}), body,
      arrayBuffer: async () => new Response(await read()).arrayBuffer(),
      text: async () => new Response(await read()).text() };
  }
  private async remoteHead(key: string) {
    try { return await this.client.send(new HeadObjectCommand({ Bucket: this.bucket, Key: this.objectKey(key) })); }
    catch (error) { if (missing(error)) return null; throw error; }
  }
  async put(key: string, value: Parameters<FileStore['put']>[1], options: Parameters<FileStore['put']>[2] = {}) {
    return this.protect([key], async () => {
      validKey(key);
      if (key.startsWith('legal/') && options.onlyIf?.etagDoesNotMatch !== '*') throw new Error('LEGAL_DOCUMENT_IMMUTABLE');
      if (options.onlyIf && options.onlyIf.etagDoesNotMatch !== '*') throw new Error('UNSUPPORTED_OBJECT_CONDITION');
      if (options.onlyIf && await this.head(key)) return null;
      const source = await uploadSource(value), etag = source.checksum;
      try {
        await this.client.send(new PutObjectCommand({ Bucket: this.bucket, Key: this.objectKey(key), Body: source.body, ContentLength: source.size, ContentType: options.httpMetadata?.contentType, CacheControl: options.httpMetadata?.cacheControl, ContentDisposition: options.httpMetadata?.contentDisposition, Metadata: { 'erpx-key': Buffer.from(key).toString('base64url'), 'erpx-sha256': etag, 'erpx-http': JSON.stringify(options.httpMetadata ?? {}), 'erpx-custom': JSON.stringify(options.customMetadata ?? {}) }, ...(options.onlyIf ? { IfNoneMatch: '*' } : {}) }));
      } catch (error) { if (options.onlyIf && (error as { $metadata?: { httpStatusCode?: number } }).$metadata?.httpStatusCode === 412) return null; throw error; }
      finally { await source.cleanup(); }
      return this.head(key);
    });
  }
  async delete(input: string | string[], authorization?: Parameters<FileStore['delete']>[1]) {
    const keys = Array.isArray(input) ? input : [input];
    if (keys.some(key => key.startsWith('legal/'))) throw new Error('LEGAL_DOCUMENT_PRESERVATION_REQUIRED');
    // Delete the fallback first. If it fails, the remote object must remain
    // readable; otherwise the fallback can resurrect a successfully deleted
    // remote object. Both stores recheck preservation on the same owning
    // transaction, without acquiring another connection or advisory lock owner.
    await this.protect(keys, async tx => {
      if (this.legacy) await this.legacy.delete(keys, this.legacy.supportsTransactionDeletion && tx ? { database: tx } : undefined);
      for (const key of keys) await this.client.send(new DeleteObjectCommand({ Bucket: this.bucket, Key: this.objectKey(key) }));
    }, authorization?.database);
  }
  private async protect<T>(keys: string[], operation: (tx?: SqlDatabase) => Promise<T>, database?: SqlDatabase): Promise<T> {
    if (database) {
      if (!this.preservationDb) throw new Error('TRANSACTION_NOT_ACTIVE_OR_OWNED');
      this.preservationDb.assertActiveTransaction(database);
    }
    if (!this.preservationDb) return operation();
    const guarded = async (tx: SqlDatabase) => {
      const authority=(await import("@erpx/server-legal/public/runtime-minio-preservation-authority")).runtimeMinioPreservationAuthority(tx);
      await authority.lock();
      if(await authority.preservationReady())for(const key of keys)
        if(await authority.pendingOwner(key))throw new Error('FILE_PRESERVED_PENDING_OWNER');
      for(const key of keys)if(await authority.held(key))throw new Error('LEGAL_HOLD_BLOCKS_FILE_MUTATION');
      return operation(tx);
    };
    return database ? guarded(database) : this.preservationDb.transaction(guarded);
  }
  async destroyPreserved(keys: string[], authorization: Parameters<NonNullable<FileStore['destroyPreserved']>>[1]) {
    const row=await (await import("@erpx/server-legal/public/runtime-minio-preservation-authority")).runtimeMinioPreservationAuthority(authorization.database).destruction(authorization.destructionId);
    if (!row) throw new Error('LEGAL_DESTRUCTION_NOT_AUTHORIZED');
    const manifest = typeof row.manifest === 'string' ? JSON.parse(row.manifest) : row.manifest;
    for (const key of keys) {
      if (!key.startsWith('legal/') || !manifest.keys.includes(key)) throw new Error('LEGAL_DESTRUCTION_SCOPE');
      await this.client.send(new DeleteObjectCommand({ Bucket: this.bucket, Key: this.objectKey(key) }));
      await this.legacy?.destroyPreserved?.([key], authorization);
    }
  }
  async list(options: Parameters<FileStore['list']>[0] = {}) {
    const found = new Map<string, FileMetadata>();
    let token: string | undefined;
    do {
      const page = await this.client.send(new ListObjectsV2Command({ Bucket: this.bucket, Prefix: `${this.prefix}/`, ContinuationToken: token }));
      for (const object of page.Contents ?? []) {
        if (!object.Key) continue;
        const result = await this.client.send(new HeadObjectCommand({ Bucket: this.bucket, Key: object.Key }));
        const key = result.Metadata?.['erpx-key'];
        if (!key) continue;
        const decoded = Buffer.from(key, 'base64url').toString();
        if (object.Key !== this.objectKey(decoded)) throw new Error('INVALID_OBJECT_MANIFEST');
        const meta = this.metadata(decoded, result);
        if (decoded.startsWith(options.prefix ?? '') && (!options.cursor || decoded > options.cursor)) found.set(decoded, meta);
      }
      token = page.NextContinuationToken;
    } while (token);
    if (this.legacy) {
      let cursor: string | undefined;
      do { const page = await this.legacy.list({ prefix: options.prefix, cursor, limit: 1000 }); for (const item of page.objects) if ((!options.cursor || item.key > options.cursor) && !found.has(item.key)) found.set(item.key, item); cursor = page.cursor; } while (cursor);
    }
    const limit = Math.max(1, Math.min(1000, options.limit ?? 1000));
    const objects = [...found.values()].sort((a, b) => a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
    return { objects: objects.slice(0, limit), truncated: objects.length > limit, ...(objects.length > limit ? { cursor: objects[limit - 1].key } : {}) };
  }
}
