import { createWriteStream } from 'node:fs';
import { chmod, mkdir, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pipeline } from 'node:stream/promises';
import { Client } from 'minio';
import { config } from '../lib/config';

type ObjectInfo = {
  name: string;
  size: number;
  etag: string;
};

const client = new Client({
  endPoint: config.storage.minio.endPoint,
  port: config.storage.minio.port,
  useSSL: config.storage.minio.useSSL,
  accessKey: config.storage.minio.accessKey,
  secretKey: config.storage.minio.secretKey,
});

const bucket = config.storage.minio.bucket;

async function list(): Promise<ObjectInfo[]> {
  if (!(await client.bucketExists(bucket))) return [];
  const objects: ObjectInfo[] = [];
  const stream = client.listObjectsV2(bucket, '', true);
  await new Promise<void>((resolve, reject) => {
    stream.on('data', (item) => {
      if (!item.name) return;
      objects.push({ name: item.name, size: item.size, etag: item.etag || '' });
    });
    stream.on('error', reject);
    stream.on('end', resolve);
  });
  return objects.sort((a, b) => a.name.localeCompare(b.name));
}

function safeTarget(root: string, name: string): string {
  const target = path.resolve(root, name);
  if (!target.startsWith(`${path.resolve(root)}${path.sep}`)) {
    throw new Error(`Unsafe object key: ${name}`);
  }
  return target;
}

async function backup(destination: string): Promise<void> {
  const root = path.resolve(destination);
  await mkdir(root, { recursive: true, mode: 0o700 });
  await chmod(root, 0o700);
  const objects = await list();
  for (const object of objects) {
    const target = safeTarget(root, object.name);
    await mkdir(path.dirname(target), { recursive: true, mode: 0o700 });
    const source = await client.getObject(bucket, object.name);
    await pipeline(source, createWriteStream(target, { mode: 0o600 }));
    const copied = await stat(target);
    if (copied.size !== object.size) {
      throw new Error(`Backup size mismatch for ${object.name}`);
    }
  }
  const manifest = {
    bucket,
    createdAt: new Date().toISOString(),
    objectCount: objects.length,
    totalBytes: objects.reduce((sum, object) => sum + object.size, 0),
    objects,
  };
  const manifestPath = path.join(root, 'manifest.json');
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 });
  const copiedFiles = await Promise.all(objects.map((object) => stat(safeTarget(root, object.name))));
  const copiedBytes = copiedFiles.reduce((sum, file) => sum + file.size, 0);
  if (copiedFiles.length !== objects.length || copiedBytes !== manifest.totalBytes) {
    throw new Error('MinIO backup verification failed');
  }
  process.stdout.write(`Verified MinIO backup: ${objects.length} objects, ${manifest.totalBytes} bytes.\n`);
}

async function recreate(): Promise<void> {
  if (await client.bucketExists(bucket)) {
    const objects = await list();
    for (let index = 0; index < objects.length; index += 1000) {
      await client.removeObjects(bucket, objects.slice(index, index + 1000).map((object) => object.name));
    }
    if ((await list()).length !== 0) throw new Error('MinIO bucket is not empty');
    await client.removeBucket(bucket);
  }
  await client.makeBucket(bucket, 'us-east-1');
  if (!(await client.bucketExists(bucket)) || (await list()).length !== 0) {
    throw new Error('MinIO bucket recreation verification failed');
  }
  process.stdout.write(`Recreated empty MinIO bucket: ${bucket}.\n`);
}

const [mode, destination] = process.argv.slice(2);
if (mode === 'backup' && destination) await backup(destination);
else if (mode === 'recreate') await recreate();
else throw new Error('Usage: bun db/storage-reset.ts backup <directory> | recreate');
