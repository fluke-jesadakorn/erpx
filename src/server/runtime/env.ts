import { readFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { z } from 'zod';
import { PostgresDatabase } from './postgres';
import { LocalFileStore } from './filesystem';
import type { SqlDatabase, FileStore } from './types';
import type { BetaEnvironment } from '../beta-config';
import type { ResourceSlot } from '../../lib/holding-registry';

export type DatabaseManifest = { version: 1; databases: Record<string, string>; slots: ResourceSlot[] };
export type RuntimeEnv = BetaEnvironment & {
  ERPX_ENVIRONMENT: string;
  ERPX_PUBLIC_ORIGIN: string;
  ERPX_DATA_DIR: string;
  BETTER_AUTH_URL: string;
  BETTER_AUTH_SECRET: string;
  CONTROL_DB: SqlDatabase;
  CONTROL_READ_DB: SqlDatabase;
  H001_DB: SqlDatabase; H001_READ_DB: SqlDatabase; H001_FILES: FileStore;
  H002_DB: SqlDatabase; H002_READ_DB: SqlDatabase; H002_FILES: FileStore;
  [key: string]: unknown;
};
const manifestSchema = z.object({ version: z.literal(1), databases: z.record(z.string(), z.string().url()),
  slots: z.array(z.object({ id: z.string().regex(/^[a-z0-9]{3,24}$/), write: z.string(), read: z.string(), files: z.string(), beta: z.boolean().optional() })).max(22) });
const runtimeGlobal = globalThis as typeof globalThis & { erpxDevRuntime?: { key: string; env: RuntimeEnv } };
let singleton: RuntimeEnv | undefined;
function runtimeKey() { return JSON.stringify([process.env.ERPX_DATABASE_MANIFEST, process.env.ERPX_PROCESS_ROLE, process.env.ERPX_DEV_PG_POOL_MAX, process.env.ERPX_PUBLIC_ORIGIN, process.env.ERPX_DATA_DIR]); }

export function getRuntime(): RuntimeEnv {
  if (singleton) return singleton;
  if (process.env.NODE_ENV === 'development' && runtimeGlobal.erpxDevRuntime?.key === runtimeKey()) return singleton = runtimeGlobal.erpxDevRuntime.env;
  if (process.env.NODE_ENV === 'development' && runtimeGlobal.erpxDevRuntime) {
    void closeDatabases(runtimeGlobal.erpxDevRuntime.env).catch(() => {});
    delete runtimeGlobal.erpxDevRuntime;
  }
  const production = process.env.NODE_ENV === 'production';
  const data = resolve(/* turbopackIgnore: true */ process.env.ERPX_DATA_DIR ?? '.runtime/local');
  if (production && !isAbsolute(process.env.ERPX_DATA_DIR ?? '')) throw new Error('ERPX_DATA_DIR_REQUIRED');
  const origin = new URL(process.env.ERPX_PUBLIC_ORIGIN ?? 'http://localhost:3100');
  if (origin.pathname !== '/' || origin.search || origin.hash || origin.username || origin.password || (production && origin.protocol !== 'https:')) throw new Error('INVALID_PUBLIC_ORIGIN');
  const secret = process.env.BETTER_AUTH_SECRET ?? '';
  if (secret.length < 32 || /^(generate-|local-development)/.test(secret)) throw new Error('BETTER_AUTH_SECRET_REQUIRED');
  // The manifest is installed outside the release. A mandatory environment path
  // also prevents Next's file tracer from including the development workspace.
  const manifestPath = process.env.ERPX_DATABASE_MANIFEST;
  if (!manifestPath || !isAbsolute(manifestPath)) throw new Error('ERPX_DATABASE_MANIFEST_REQUIRED');
  const manifest = manifestSchema.parse(JSON.parse(readFileSync(manifestPath, 'utf8')));
  const publicBeta = process.env.ERPX_PUBLIC_BETA === 'true';
  if (production && publicBeta && (!process.env.RESEND_API_KEY || !process.env.RESEND_FROM)) throw new Error('REGISTRATION_EMAIL_NOT_CONFIGURED');
  const env: Record<string, unknown> = {
    ERPX_ENVIRONMENT: production ? 'production' : process.env.ERPX_ENVIRONMENT ?? 'local',
    ERPX_PUBLIC_ORIGIN: origin.origin, ERPX_DATA_DIR: data,
    BETTER_AUTH_URL: origin.origin, BETTER_AUTH_SECRET: secret,
    ERPX_RESOURCE_SLOTS: JSON.stringify(manifest.slots), ERPX_PUBLIC_BETA: String(publicBeta),
  };
  for (const name of ['BETA_MEMBER_LIMIT','BETA_BRANCH_LIMIT','BETA_AI_LIMIT','BETA_STORAGE_BYTES','RESEND_API_KEY','RESEND_FROM',
    'ERPX_IAM_SECRET_PROFILES','AI_BASE_URL','AI_MODEL','AI_API_KEY','AI_CREDENTIAL_KEY_ID','AI_CREDENTIAL_KEYS','STRIPE_SECRET_KEY','STRIPE_WEBHOOK_SECRET','STRIPE_PRICE_ID'])
    if (process.env[name]) env[name] = process.env[name];
  const role = process.env.ERPX_PROCESS_ROLE === 'jobs' ? 'jobs' : 'app';
  const devPool = Number(process.env.ERPX_DEV_PG_POOL_MAX ?? 6);
  if (!Number.isInteger(devPool) || devPool < 1 || devPool > 16) throw new Error('INVALID_DEV_PG_POOL_MAX');
  for (const [binding, connectionString] of Object.entries(manifest.databases)) {
    // Installer provides independently scoped database logins; a separate jobs
    // manifest grants projection writes that the web process cannot perform.
    env[binding] = new PostgresDatabase({ connectionString, application_name: `erpx-${role}`, max: binding === 'CONTROL_DB' ? (role === 'jobs' ? 2 : 5) : (process.env.NODE_ENV === 'development' && role === 'app' ? devPool : 1) });
  }
  if (!env.CONTROL_DB) throw new Error('CONTROL_DATABASE_REQUIRED');
  for (const slot of manifest.slots) {
    if (!env[slot.write] || !env[slot.read]) throw new Error('HOLDING_DATABASE_REQUIRED');
    env[slot.files] = new LocalFileStore(join(data, 'files', slot.id), env[slot.write] as PostgresDatabase);
  }
  singleton = env as RuntimeEnv;
  if (process.env.NODE_ENV === 'development') runtimeGlobal.erpxDevRuntime = { key: runtimeKey(), env: singleton };
  return singleton;
}
function closeDatabases(env: RuntimeEnv) {
  // Constructors change during HMR; an instanceof check would miss retained pools.
  const databases = Object.values(env).filter((value): value is PostgresDatabase =>
    !!value && typeof value === 'object' && 'pool' in value && 'close' in value && typeof value.close === 'function');
  return Promise.all(databases.map(db => db.close()));
}
export async function closeRuntime() {
  if (!singleton) return;
  const current = singleton; singleton = undefined;
  if (runtimeGlobal.erpxDevRuntime?.env === current) delete runtimeGlobal.erpxDevRuntime;
  await closeDatabases(current);
}
