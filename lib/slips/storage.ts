// Storage adapter for slip files.
// MinIO-backed implementation; interface is S3-shaped so the backend is
// swappable without caller-side changes.

import { Client as MinioClient } from 'minio';
import { Readable } from 'node:stream';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { config } from '../config';

const minioClient = new MinioClient({
  endPoint: config.storage.minio.publicHost || config.storage.minio.endPoint,
  port: config.storage.minio.publicPort > 0 ? config.storage.minio.publicPort : config.storage.minio.port,
  useSSL: config.storage.minio.useSSL,
  accessKey: config.storage.minio.accessKey,
  secretKey: config.storage.minio.secretKey,
});

export { minioClient };

let bucketReady = false;
async function ensureBucket(): Promise<void> {
  if (bucketReady) return;
  const exists = await minioClient.bucketExists(config.storage.minio.bucket).catch(() => false);
  if (!exists) {
    await minioClient.makeBucket(config.storage.minio.bucket, 'us-east-1');
  }
  bucketReady = true;
}

function datePrefix(): string {
  const d = new Date();
  return `${d.getUTCFullYear()}/${String(d.getUTCMonth() + 1).padStart(2, '0')}`;
}

export async function put(key: string, buffer: Buffer, contentType?: string): Promise<{ backend: string; key: string; size: number }> {
  await ensureBucket();
  await minioClient.putObject(config.storage.minio.bucket, key, buffer, buffer.length, {
    'Content-Type': contentType || 'application/octet-stream',
  });
  return { backend: 'minio', key, size: buffer.length };
}

export async function get(key: string): Promise<Buffer> {
  await ensureBucket();
  const stream = await minioClient.getObject(config.storage.minio.bucket, key);
  return await streamToBuffer(stream);
}

export async function exists(key: string): Promise<boolean> {
  await ensureBucket();
  try {
    await minioClient.statObject(config.storage.minio.bucket, key);
    return true;
  } catch {
    return false;
  }
}

export async function remove(key: string): Promise<void> {
  await ensureBucket();
  await minioClient.removeObject(config.storage.minio.bucket, key).catch(() => {});
}

export function getStream(key: string): Readable {
  const obj = minioClient.getObject(config.storage.minio.bucket, key);
  return obj as unknown as Readable;
}

export async function presignedGetUrl(key: string, expirySeconds = 600): Promise<string> {
  await ensureBucket();
  const signed = await (minioClient.presignedGetObject as (b: string, k: string, e: number) => Promise<string>)(
    config.storage.minio.bucket, key, expirySeconds,
  );
  return rewritePublicHost(signed);
}

export async function presignedPutUrl(key: string, expirySeconds = 600): Promise<string> {
  await ensureBucket();
  const signed = await (minioClient.presignedPutObject as (b: string, k: string, e: number) => Promise<string>)(
    config.storage.minio.bucket, key, expirySeconds,
  );
  return rewritePublicHost(signed);
}

function rewritePublicHost(signedUrl: string): string {
  const { publicHost, publicPort } = config.storage.minio;
  if (!publicHost) return signedUrl;
  const u = new URL(signedUrl);
  u.hostname = publicHost;
  if (publicPort > 0) u.port = String(publicPort);
  else u.port = '';
  return u.toString();
}

export function makeKey(originalName: string): string {
  const ext = path.extname(originalName || '').toLowerCase() || '.bin';
  const id = randomUUID();
  return `${datePrefix()}/${id}${ext}`;
}

export function publicUrlFor(key: string): string {
  return `/api/slips/file?key=${encodeURIComponent(key)}`;
}

export function filePathFromKey(key: string): string {
  return key;
}

function streamToBuffer(stream: Readable): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    stream.on('data', (c: Buffer | string) => chunks.push(Buffer.isBuffer(c) ? c : Buffer.from(c)));
    stream.on('end', () => resolve(Buffer.concat(chunks)));
    stream.on('error', reject);
  });
}

export function readIntoBuffer(stream: Readable): Promise<Buffer> {
  return streamToBuffer(stream);
}
