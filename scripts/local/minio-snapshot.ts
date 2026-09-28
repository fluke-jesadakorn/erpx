import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, mkdir, open, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { Readable } from 'node:stream';
import { GetObjectCommand, HeadObjectCommand, ListObjectsV2Command, PutObjectCommand, S3Client } from '@aws-sdk/client-s3';

type Entry = { key: string; file: string; size: number; sha256: string; contentType?: string; metadata: Record<string, string> };
type Manifest = { version: 1; bucket: string; prefix: string; objects: Entry[] };

const [action, directory, expectedEnvironment] = process.argv.slice(2);
if (!['backup', 'verify', 'restore'].includes(action) || !directory || !['production', 'staging', 'local'].includes(expectedEnvironment)) {
  throw new Error('MINIO_SNAPSHOT_ARGUMENTS_REQUIRED');
}
const environment = process.env.NODE_ENV === 'production' && process.env.ERPX_ENVIRONMENT !== 'staging'
  ? 'production' : process.env.ERPX_ENVIRONMENT ?? 'local';
if (environment !== expectedEnvironment) throw new Error('MINIO_ENVIRONMENT_MISMATCH');
const endpoint = process.env.ERPX_MINIO_ENDPOINT;
const bucket = process.env.ERPX_MINIO_BUCKET ?? '';
const accessKeyId = process.env.ERPX_MINIO_ACCESS_KEY;
const secretAccessKey = process.env.ERPX_MINIO_SECRET_KEY;
if (!endpoint || !bucket || !accessKeyId || !secretAccessKey) throw new Error('ERPX_MINIO_CONFIG_INCOMPLETE');
const endpointUrl = new URL(endpoint);
if (!['http:', 'https:'].includes(endpointUrl.protocol) || endpointUrl.pathname !== '/' || endpointUrl.search || endpointUrl.hash || endpointUrl.username || endpointUrl.password) throw new Error('ERPX_MINIO_ENDPOINT_INVALID');
if (!/^[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]$/.test(bucket)) throw new Error('ERPX_MINIO_BUCKET_INVALID');
const prefix = `environments/${environment}/`;
const snapshot = join(directory, 'minio');
const objectDir = join(snapshot, 'objects');
const client = new S3Client({ endpoint, region: process.env.ERPX_MINIO_REGION ?? 'us-east-1', forcePathStyle: true, credentials: { accessKeyId, secretAccessKey } });

function assertObjectKey(key: string): void {
  if (!key.startsWith(prefix)) throw new Error('MINIO_OBJECT_OUTSIDE_ENVIRONMENT');
  const scoped = key.slice(prefix.length);
  if (!/^(?:holdings\/[A-Za-z0-9_-]+|platform-billing)\/[a-f0-9]{64}$/.test(scoped)) throw new Error('MINIO_OBJECT_KEY_INVALID');
}

function objectFile(key: string): string { return createHash('sha256').update(key).digest('hex'); }

function assertMetadata(entry: Entry): void {
  assertObjectKey(entry.key);
  const hash = entry.key.slice(-64);
  const original = entry.metadata['erpx-key'];
  if (!original || !/^[A-Za-z0-9_-]+$/.test(original) || createHash('sha256').update(Buffer.from(original, 'base64url')).digest('hex') !== hash) throw new Error('MINIO_OBJECT_MANIFEST_INVALID');
  if (entry.metadata['erpx-sha256'] !== entry.sha256) throw new Error('MINIO_OBJECT_CONTENT_MISMATCH');
  for (const key of ['erpx-http', 'erpx-custom']) {
    if (typeof entry.metadata[key] !== 'string') throw new Error('MINIO_OBJECT_MANIFEST_INVALID');
    const value: unknown = JSON.parse(entry.metadata[key]);
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('MINIO_OBJECT_MANIFEST_INVALID');
  }
}

async function sha256File(path: string): Promise<{ sha256: string; size: number }> {
  const hash = createHash('sha256');
  let size = 0;
  for await (const chunk of createReadStream(path)) { const bytes = Buffer.from(chunk); hash.update(bytes); size += bytes.length; }
  return { sha256: hash.digest('hex'), size };
}

async function writeObject(key: string, file: string): Promise<Entry> {
  const response = await client.send(new GetObjectCommand({ Bucket: bucket, Key: key }));
  const stream = response.Body?.transformToWebStream();
  if (!stream) throw new Error('MINIO_OBJECT_BODY_MISSING');
  const metadata = response.Metadata ?? {};
  const hash = createHash('sha256');
  let size = 0;
  const output = await open(file, 'wx', 0o600);
  try {
    for await (const chunk of Readable.fromWeb(stream as never)) {
      const bytes = Buffer.from(chunk);
      hash.update(bytes); size += bytes.length;
      let offset = 0;
      while (offset < bytes.length) offset += (await output.write(bytes, offset, bytes.length - offset)).bytesWritten;
    }
    await output.sync();
  } finally { await output.close(); }
  const entry = { key, file: objectFile(key), size, sha256: hash.digest('hex'), contentType: response.ContentType, metadata };
  assertMetadata(entry);
  if (response.ContentLength !== undefined && response.ContentLength !== size) throw new Error('MINIO_OBJECT_SIZE_MISMATCH');
  return entry;
}

async function readManifest(): Promise<Manifest> {
  const manifest = JSON.parse(await readFile(join(snapshot, 'manifest.json'), 'utf8')) as Manifest;
  if (manifest.version !== 1 || manifest.bucket !== bucket || manifest.prefix !== prefix || !Array.isArray(manifest.objects)) throw new Error('MINIO_SNAPSHOT_SCOPE_MISMATCH');
  const seen = new Set<string>();
  for (const entry of manifest.objects) {
    if (!entry || typeof entry.key !== 'string' || entry.file !== objectFile(entry.key) || typeof entry.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(entry.sha256) || !Number.isSafeInteger(entry.size) || entry.size < 0 || !entry.metadata || typeof entry.metadata !== 'object' || Array.isArray(entry.metadata) || (entry.contentType !== undefined && typeof entry.contentType !== 'string') || Object.values(entry.metadata).some(value => typeof value !== 'string')) throw new Error('MINIO_SNAPSHOT_MANIFEST_INVALID');
    assertMetadata(entry);
    if (seen.has(entry.key)) throw new Error('MINIO_SNAPSHOT_DUPLICATE_OBJECT');
    seen.add(entry.key);
    const file = join(objectDir, entry.file);
    const objectStat = await lstat(file);
    if (!objectStat.isFile() || objectStat.isSymbolicLink()) throw new Error('MINIO_SNAPSHOT_OBJECT_INVALID');
    const actual = await sha256File(file);
    if (actual.sha256 !== entry.sha256 || actual.size !== entry.size) throw new Error('MINIO_SNAPSHOT_OBJECT_MISMATCH');
  }
  return manifest;
}

async function main(): Promise<void> {
  try {
    if (action === 'backup') {
      await mkdir(objectDir, { recursive: true, mode: 0o700 });
      const objects: Entry[] = [];
      let token: string | undefined;
      do {
        const page = await client.send(new ListObjectsV2Command({ Bucket: bucket, Prefix: prefix, ContinuationToken: token }));
        for (const item of page.Contents ?? []) {
          if (!item.Key) throw new Error('MINIO_OBJECT_KEY_MISSING');
          assertObjectKey(item.Key);
          objects.push(await writeObject(item.Key, join(objectDir, objectFile(item.Key))));
        }
        token = page.NextContinuationToken;
      } while (token);
      objects.sort((a, b) => a.key.localeCompare(b.key));
      const manifest: Manifest = { version: 1, bucket, prefix, objects };
      await writeFile(join(snapshot, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
      await readManifest();
      return;
    }
    const manifest = await readManifest();
    if (action === 'restore') {
      for (const entry of manifest.objects) {
        await client.send(new PutObjectCommand({ Bucket: bucket, Key: entry.key, Body: createReadStream(join(objectDir, entry.file)), ContentLength: entry.size, ContentType: entry.contentType, Metadata: entry.metadata }));
        const restored = await client.send(new HeadObjectCommand({ Bucket: bucket, Key: entry.key }));
        if (restored.ContentLength !== entry.size || restored.Metadata?.['erpx-sha256'] !== entry.sha256) throw new Error('MINIO_RESTORE_VERIFY_FAILED');
      }
    }
  } finally { client.destroy(); }
}

await main();
