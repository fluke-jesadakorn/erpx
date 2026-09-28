/**
 * Copy the current local FileStore objects into the already provisioned MinIO
 * bucket. Local bytes are never deleted. Run while file writes are quiesced:
 *
 *   bun scripts/local/migrate-files-to-minio.ts --plan --data-dir /absolute/data --environment local
 *   bun scripts/local/migrate-files-to-minio.ts --apply --expect-count N --data-dir /absolute/data --environment local
 *   bun scripts/local/migrate-files-to-minio.ts --verify --expect-count N --data-dir /absolute/data --environment local
 *
 * ERPX_DATABASE_MANIFEST and ERPX_MINIO_* must point at the same deployment.
 * The destination key layout must match the runtime file-store constructors.
 */
import { createHash } from 'node:crypto';
import { readFile, readdir, lstat } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import { ListObjectsV2Command, S3Client } from '@aws-sdk/client-s3';
import { Pool, type QueryResult } from 'pg';
import { LocalFileStore } from '../../src/server/runtime/filesystem';
import { MinioFileStore } from '../../src/server/runtime/minio';
import type { FileMetadata, FileStore } from '../../src/server/runtime/types';

type Mode = 'plan' | 'apply' | 'verify';
type Store = { name: string; local: LocalFileStore; remote: MinioFileStore; prefix: string };
type Inventory = { objects: FileMetadata[]; bytes: number };
type ManifestSlot = { id: string; write: string };
type DatabaseManifest = { slots: ManifestSlot[]; databases: Record<string, unknown> };
type ReferenceSource = { table: string; keyExpression: string };

const holdingReferences: ReferenceSource[] = [
  { table: 'attachments', keyExpression: 'object_key' },
  { table: 'attachment_derivatives', keyExpression: 'object_key' },
  { table: 'upload_intents', keyExpression: 'final_key' },
  { table: 'messaging_files', keyExpression: 'object_key' },
  { table: 'legal_documents', keyExpression: 'object_key' },
  { table: 'legal_external_uploads', keyExpression: 'object_key' },
  { table: 'legal_file_preservations', keyExpression: 'object_key' },
  { table: 'hr_payment_batches', keyExpression: 'file_key' },
  { table: 'hr_payroll_review_exports', keyExpression: 'file_key' },
];
const billingReferences: ReferenceSource[] = [
  { table: 'billing_evidence', keyExpression: "holding_id || '/' || id" },
];

const sha256 = (value: Buffer | string) => createHash('sha256').update(value).digest('hex');
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
class MigrationFailure extends Error {}
function fail(message: string): never { throw new MigrationFailure(message); }

function usage() {
  console.log('Usage: bun scripts/local/migrate-files-to-minio.ts --plan|--apply|--verify --data-dir ABSOLUTE_PATH --environment NAME [--expect-count N]');
  console.log('--expect-count is required for --apply and --verify. Get N from --plan after quiescing file writes.');
  console.log('Requires matching ERPX_DATA_DIR and ERPX_ENVIRONMENT, plus ERPX_DATABASE_MANIFEST and ERPX_MINIO_* configuration.');
  console.log('Run with file writes quiesced. --apply never deletes local files or overwrites existing remote objects.');
  console.log('--apply and --verify also check database file references directly against MinIO.');
}

function argumentsFromCli() {
  const args = process.argv.slice(2);
  if (args.includes('--help')) { usage(); process.exit(0); }
  let mode: Mode | undefined;
  let dataDir: string | undefined;
  let environment: string | undefined;
  let expectedCount: number | undefined;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--plan' || arg === '--apply' || arg === '--verify') {
      if (mode) fail('Specify exactly one mode');
      mode = arg.slice(2) as Mode;
    } else if (arg === '--data-dir' || arg === '--environment' || arg === '--expect-count') {
      const value = args[++i];
      if (!value || value.startsWith('--')) fail(`Missing value for ${arg}`);
      if (arg === '--data-dir') dataDir = value;
      else if (arg === '--environment') environment = value;
      else {
        if (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) fail('Invalid expected count');
        expectedCount = Number(value);
      }
    } else fail(`Unknown option: ${arg}`);
  }
  if (!mode || !dataDir || !environment) { usage(); fail('Mode, data dir and environment are required'); }
  if (!isAbsolute(dataDir)) fail('Data dir must be absolute');
  if (!/^[a-z][a-z0-9-]{0,31}$/.test(environment)) fail('Invalid environment name');
  if (!process.env.ERPX_DATA_DIR || resolve(process.env.ERPX_DATA_DIR) !== resolve(dataDir))
    fail('Data dir must match ERPX_DATA_DIR');
  if (!process.env.ERPX_ENVIRONMENT || process.env.ERPX_ENVIRONMENT !== environment)
    fail('Environment must match ERPX_ENVIRONMENT');
  if (mode !== 'plan' && expectedCount === undefined) fail('--expect-count is required for apply and verify');
  return { mode, dataDir: resolve(dataDir), environment, expectedCount };
}

async function configuredSlots() {
  const manifestPath = process.env.ERPX_DATABASE_MANIFEST;
  if (!manifestPath || !isAbsolute(manifestPath)) fail('Absolute ERPX_DATABASE_MANIFEST is required');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as {
    slots?: Array<{ id?: unknown; write?: unknown }>;
    databases?: Record<string, unknown>;
  };
  if (!Array.isArray(manifest.slots) || manifest.slots.length === 0) fail('Invalid database manifest slots');
  const slots = manifest.slots.map(slot => ({ id: slot.id, write: slot.write }));
  if (slots.some(slot => typeof slot.id !== 'string' || !/^[a-z0-9]{3,24}$/.test(slot.id)
    || typeof slot.write !== 'string' || !/^[A-Z][A-Z0-9_]*$/.test(slot.write)))
    fail('Invalid holding slot in database manifest');
  if (new Set(slots.map(slot => slot.id)).size !== slots.length) fail('Duplicate holding ID in database manifest');
  if (!manifest.databases || typeof manifest.databases !== 'object' || Array.isArray(manifest.databases))
    fail('Invalid database connections in manifest');
  return { slots: slots as ManifestSlot[], databases: manifest.databases } satisfies DatabaseManifest;
}

function minioConfig() {
  const { ERPX_MINIO_ENDPOINT: endpoint, ERPX_MINIO_BUCKET: bucket,
    ERPX_MINIO_ACCESS_KEY: accessKey, ERPX_MINIO_SECRET_KEY: secretKey,
    ERPX_MINIO_REGION: region = 'us-east-1' } = process.env;
  if (!endpoint || !bucket || !accessKey || !secretKey) fail('ERPX_MINIO_CONFIG_INCOMPLETE');
  const url = new URL(endpoint);
  if (!['http:', 'https:'].includes(url.protocol) || url.pathname !== '/' || url.search || url.hash || url.username || url.password)
    fail('ERPX_MINIO_ENDPOINT_INVALID');
  if (!/^[a-z0-9][a-z0-9.-]{1,62}$/.test(bucket)) fail('ERPX_MINIO_BUCKET_INVALID');
  return { endpoint, bucket, accessKey, secretKey, region };
}

async function assertKnownLocalRoots(dataDir: string, slotIds: string[]) {
  const allowed = new Set([...slotIds, 'platform-billing']);
  const root = join(dataDir, 'files');
  let entries;
  try { entries = await readdir(root, { withFileTypes: true }); }
  catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') fail('Local files root missing'); throw error; }
  for (const entry of entries) {
    if (!entry.isDirectory() || !allowed.has(entry.name)) fail('Unexpected local file-store entry');
  }
}

async function listAll(store: FileStore): Promise<FileMetadata[]> {
  const objects: FileMetadata[] = [];
  let cursor: string | undefined;
  do {
    const page = await store.list({ cursor, limit: 1000 });
    objects.push(...page.objects);
    if (page.truncated && (!page.cursor || page.cursor === cursor)) fail('File-store pagination did not advance');
    cursor = page.cursor;
  } while (cursor);
  return objects;
}

async function localInventory(store: LocalFileStore): Promise<Inventory> {
  const directories = await readdir(store.root, { withFileTypes: true }).catch((error: NodeJS.ErrnoException) => {
    if (error.code === 'ENOENT') return [];
    throw error;
  });
  const keys = new Set<string>();
  for (const directory of directories) {
    if (!directory.isDirectory() || !/^[a-f0-9]{64}$/.test(directory.name))
      fail('Unexpected entry in local file store');
    const base = join(store.root, directory.name);
    const manifestPath = join(base, 'current.json');
    const manifestStat = await lstat(manifestPath);
    if (!manifestStat.isFile()) fail('Invalid local manifest');
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8')) as {
      key?: unknown; version?: unknown; size?: unknown; etag?: unknown;
    };
    if (typeof manifest.key !== 'string' || !manifest.key || sha256(manifest.key) !== directory.name
      || typeof manifest.version !== 'string' || !/^[a-f0-9-]{36}$/.test(manifest.version)
      || !Number.isSafeInteger(manifest.size) || (manifest.size as number) < 0
      || typeof manifest.etag !== 'string' || !/^[a-f0-9]{64}$/.test(manifest.etag))
      fail('Invalid local manifest');
    const body = await lstat(join(base, manifest.version));
    if (!body.isFile() || body.size !== manifest.size) fail('Invalid local body');
    if (keys.has(manifest.key)) fail('Duplicate local object key');
    keys.add(manifest.key);
  }
  const objects = await listAll(store);
  if (objects.length !== keys.size || objects.some(object => !keys.has(object.key)))
    fail('Local inventory mismatch');
  return { objects, bytes: objects.reduce((sum, object) => sum + object.size, 0) };
}

function assertMetadata(expected: FileMetadata, actual: FileMetadata | null, label: string) {
  if (!actual || actual.key !== expected.key || actual.size !== expected.size || actual.etag !== expected.etag
    || !same(actual.httpMetadata, expected.httpMetadata) || !same(actual.customMetadata, expected.customMetadata))
    fail(`${label}: metadata or checksum differs`);
}

async function readAndHash(store: FileStore, object: FileMetadata, label: string): Promise<Buffer> {
  const file = await store.get(object.key);
  assertMetadata(object, file, label);
  const bytes = Buffer.from(await file!.arrayBuffer());
  if (bytes.length !== object.size || sha256(bytes) !== object.etag)
    fail(`${label}: byte checksum differs`);
  return bytes;
}

async function rawRemoteKeys(client: S3Client, bucket: string, prefix: string) {
  const keys = new Set<string>();
  let token: string | undefined;
  do {
    const page = await client.send(new ListObjectsV2Command({ Bucket: bucket, Prefix: `${prefix}/`, ContinuationToken: token }));
    for (const object of page.Contents ?? []) {
      if (!object.Key || keys.has(object.Key)) fail(`Invalid or duplicate remote key under ${prefix}`);
      keys.add(object.Key);
    }
    if (page.IsTruncated && (!page.NextContinuationToken || page.NextContinuationToken === token))
      fail(`MinIO pagination did not advance under ${prefix}`);
    token = page.NextContinuationToken;
  } while (token);
  return keys;
}

async function verifyStore(store: Store, inventory: Inventory, client: S3Client, bucket: string) {
  const expected = new Map(inventory.objects.map(object => [object.key, object]));
  const remote = await listAll(store.remote);
  if (remote.length !== expected.size) fail(`${store.name}: remote object count ${remote.length} differs from local ${expected.size}`);
  for (const object of remote) {
    const source = expected.get(object.key);
    if (!source) fail(`${store.name}: unexpected remote object`);
    assertMetadata(source, object, store.name);
    await readAndHash(store.remote, source, `${store.name} remote`);
  }
  const raw = await rawRemoteKeys(client, bucket, store.prefix);
  if (raw.size !== expected.size) fail(`${store.name}: raw MinIO object count ${raw.size} differs from local ${expected.size}`);
  for (const key of expected.keys()) {
    if (!raw.has(`${store.prefix}/${sha256(key)}`)) fail(`${store.name}: MinIO object missing`);
  }
}

function databaseConnection(manifest: DatabaseManifest, binding: string) {
  const value = manifest.databases[binding];
  if (typeof value !== 'string' || !/^postgres(?:ql)?:\/\//.test(value))
    fail(`Database connection missing for ${binding}`);
  return value;
}

async function verifyReferenceTable(
  db: Pool, namespace: string, source: ReferenceSource, prefix: string, remoteKeys: Set<string>,
) {
  const exists = await db.query<{ present: boolean }>('SELECT to_regclass($1) IS NOT NULL AS present', [source.table]);
  if (!exists.rows[0]?.present) {
    console.log(`${namespace}.${source.table}: references=0, unique=0, missing=0 (table absent)`);
    return 0;
  }
  // Table names and key expressions come only from the fixed lists above.
  const count = await db.query<{ reference_count: string }>(
    `SELECT count(*) AS reference_count FROM ${source.table} WHERE ${source.keyExpression} IS NOT NULL`,
  );
  const references = Number(count.rows[0]?.reference_count);
  if (!Number.isSafeInteger(references)) fail(`${namespace}.${source.table}: reference count out of range`);
  let cursor: string | null = null, unique = 0, missing = 0;
  for (;;) {
    const after: string = cursor === null ? '' : ` AND ${source.keyExpression} > $1`;
    const page: QueryResult<{ object_key: string }> = await db.query<{ object_key: string }>(
      `SELECT DISTINCT ${source.keyExpression} AS object_key FROM ${source.table}
       WHERE ${source.keyExpression} IS NOT NULL${after} ORDER BY object_key LIMIT 1000`,
      cursor === null ? [] : [cursor],
    );
    if (!page.rows.length) break;
    for (const row of page.rows) {
      unique++;
      if (typeof row.object_key !== 'string' || !row.object_key
        || !remoteKeys.has(`${prefix}/${sha256(row.object_key)}`)) missing++;
    }
    cursor = page.rows.at(-1)!.object_key;
  }
  console.log(`${namespace}.${source.table}: references=${references}, unique=${unique}, missing=${missing}`);
  return missing;
}

async function verifyReferenceDatabase(
  namespace: string, connectionString: string, sources: ReferenceSource[],
  prefix: string, remoteKeys: Set<string>,
) {
  const db = new Pool({ connectionString, max: 1, connectionTimeoutMillis: 5_000,
    options: '-c default_transaction_read_only=on -c statement_timeout=30000 -c lock_timeout=10000',
    application_name: 'erpx-minio-reference-verify' });
  try {
    let missing = 0;
    for (const source of sources) {
      try { missing += await verifyReferenceTable(db, namespace, source, prefix, remoteKeys); }
      catch { fail(`${namespace}.${source.table}: database reference scan failed`); }
    }
    return missing;
  } finally { await db.end(); }
}

async function verifyDatabaseReferences(
  manifest: DatabaseManifest, stores: Store[], remoteKeys: Map<string, Set<string>>,
) {
  let missing = 0;
  for (const slot of manifest.slots) {
    const store = stores.find(item => item.name === slot.id)!;
    missing += await verifyReferenceDatabase(slot.id, databaseConnection(manifest, slot.write),
      holdingReferences, store.prefix, remoteKeys.get(slot.id)!);
  }
  const billing = stores.find(item => item.name === 'platform-billing')!;
  missing += await verifyReferenceDatabase('platform-billing', databaseConnection(manifest, 'CONTROL_DB'),
    billingReferences, billing.prefix, remoteKeys.get(billing.name)!);
  console.log(`Database references missing from MinIO: ${missing}`);
  if (missing) fail('Database-referenced objects are missing from MinIO');
}

async function main() {
  const { mode, dataDir, environment, expectedCount } = argumentsFromCli();
  const manifest = await configuredSlots();
  const slotIds = manifest.slots.map(slot => slot.id);
  await assertKnownLocalRoots(dataDir, slotIds);
  const config = minioConfig();
  const names = [...slotIds, 'platform-billing'];
  const stores: Store[] = names.map(name => {
    const prefix = name === 'platform-billing'
      ? `environments/${environment}/platform-billing`
      : `environments/${environment}/holdings/${name}`;
    return { name, prefix,
      local: new LocalFileStore(join(dataDir, 'files', name)),
      // Deliberately omit the legacy fallback: head() must mean remote only.
      remote: new MinioFileStore(config.bucket, prefix, config.endpoint, config.accessKey, config.secretKey, config.region) };
  });
  const inventories = new Map<string, Inventory>();
  for (const store of stores) {
    const inventory = await localInventory(store.local);
    inventories.set(store.name, inventory);
    console.log(`${store.name}: ${inventory.objects.length} local objects, ${inventory.bytes} bytes`);
  }
  const total = [...inventories.values()].reduce((sum, inventory) => sum + inventory.objects.length, 0);
  console.log(`Target: bucket=${config.bucket}, environment=${environment}; total local objects=${total}; mode=${mode}`);
  if (expectedCount !== undefined && total !== expectedCount)
    fail(`Local object count ${total} differs from expected ${expectedCount}`);
  if (mode === 'plan') return;

  const client = new S3Client({ endpoint: config.endpoint, region: config.region, forcePathStyle: true,
    credentials: { accessKeyId: config.accessKey, secretAccessKey: config.secretKey } });
  try {
    const remoteKeys = new Map<string, Set<string>>();
    const copiedByStore = new Map<string, number>();
    // Validate all source bytes and destination namespaces before the first write.
    for (const store of stores) {
      const inventory = inventories.get(store.name)!;
      for (const object of inventory.objects) await readAndHash(store.local, object, `${store.name} local`);
      const expectedKeys = new Set(inventory.objects.map(object => `${store.prefix}/${sha256(object.key)}`));
      for (const key of await rawRemoteKeys(client, config.bucket, store.prefix))
        if (!expectedKeys.has(key)) fail(`${store.name}: unexpected object already exists in MinIO namespace`);
    }
    for (const store of stores) {
      const inventory = inventories.get(store.name)!;
      let copied = 0;
      for (const object of inventory.objects) {
        const bytes = await readAndHash(store.local, object, `${store.name} local`);
        if (mode === 'apply' && !(await store.remote.head(object.key))) {
          await store.remote.put(object.key, bytes, { onlyIf: { etagDoesNotMatch: '*' },
            httpMetadata: object.httpMetadata, customMetadata: object.customMetadata });
          copied++;
        }
      }
      copiedByStore.set(store.name, copied);
      remoteKeys.set(store.name, await rawRemoteKeys(client, config.bucket, store.prefix));
    }
    await verifyDatabaseReferences(manifest, stores, remoteKeys);
    for (const store of stores) {
      const inventory = inventories.get(store.name)!;
      await verifyStore(store, inventory, client, config.bucket);
      console.log(`${store.name}: verified ${inventory.objects.length}, copied ${copiedByStore.get(store.name)}`);
    }
    for (const store of stores) {
      const inventory = inventories.get(store.name)!;
      const finalInventory = await localInventory(store.local);
      if (!same(finalInventory.objects, inventory.objects) || finalInventory.bytes !== inventory.bytes)
        fail(`${store.name}: local inventory changed during migration; quiesce writers and rerun`);
    }
    console.log(`Verified ${total} objects in MinIO; local originals retained.`);
  } finally { client.destroy(); }
}

main().catch(error => {
  console.error(`MinIO migration failed: ${error instanceof MigrationFailure ? error.message : 'Unexpected I/O or service error'}`);
  process.exitCode = 1;
});
