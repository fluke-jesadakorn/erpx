import { Client as MinioClient } from 'minio';
import { createHash, randomUUID } from 'node:crypto';

const cfg = {
  endPoint: process.env.MINIO_ENDPOINT || 'localhost',
  port: parseInt(process.env.MINIO_PORT || '9000', 10),
  useSSL: (process.env.MINIO_USE_SSL || 'false') === 'true',
  accessKey: process.env.MINIO_ACCESS_KEY || 'minioadmin',
  secretKey: process.env.MINIO_SECRET_KEY || 'minioadmin',
  bucket: process.env.MINIO_BUCKET || 'folio-storage',
};

const c = new MinioClient(cfg);
const key = `smoke/${randomUUID()}.bin`;
const payload = Buffer.from(`folio-minio-smoke-${Date.now()}`);
const want = createHash('sha256').update(payload).digest('hex');
const t0 = Date.now();

const existsT = Date.now();
const exists = await c.bucketExists(cfg.bucket);
if (!exists) await c.makeBucket(cfg.bucket, 'us-east-1');
console.log(`bucket=${cfg.bucket} endPoint=${cfg.endPoint}:${cfg.port} exists=${exists} (${Date.now() - existsT}ms)`);

const putT = Date.now();
await c.putObject(cfg.bucket, key, payload, payload.length, { 'Content-Type': 'application/octet-stream' });
console.log(`put ${key} ${payload.length}B (${Date.now() - putT}ms)`);

const getT = Date.now();
const stream = await c.getObject(cfg.bucket, key);
const chunks = [];
for await (const ch of stream) chunks.push(Buffer.isBuffer(ch) ? ch : Buffer.from(ch));
const got = Buffer.concat(chunks);
console.log(`get ${key} ${got.length}B (${Date.now() - getT}ms)`);

const gotSha = createHash('sha256').update(got).digest('hex');
if (gotSha !== want) {
  console.error(`FAIL sha mismatch put=${want} get=${gotSha}`);
  await c.removeObject(cfg.bucket, key).catch(() => {});
  process.exit(1);
}
console.log(`sha256=${gotSha} match`);

const preT = Date.now();
const presigned = await c.presignedGetObject(cfg.bucket, key, 60);
console.log(`presigned (${Date.now() - preT}ms) url=${presigned.slice(0, 80)}...`);

const headT = Date.now();
const stat = await c.statObject(cfg.bucket, key);
console.log(`stat size=${stat.size} etag=${stat.etag} (${Date.now() - headT}ms)`);

const rmT = Date.now();
await c.removeObject(cfg.bucket, key);
console.log(`rm ${key} (${Date.now() - rmT}ms)`);

console.log(`PASS total ${Date.now() - t0}ms`);