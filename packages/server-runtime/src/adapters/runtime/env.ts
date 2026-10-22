import { readFileSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { z } from 'zod';
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import { databasePoolLimits,databasePoolSize } from "@erpx/feature-runtime/lib/database-pool-policy";
import { LocalFileStore } from "@erpx/server-runtime/adapters/runtime/filesystem";
import { MinioFileStore } from "@erpx/server-runtime/adapters/runtime/minio";
import type { SqlDatabase, FileStore } from "@erpx/server-runtime/adapters/runtime/types";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import type { ResourceSlot } from "@erpx/feature-runtime/lib/holding-registry";
import { engineeringVerificationContext,isEngineeringVerificationOrigin } from "@erpx/server-platform_administration/public/engineering-verification";

export type DatabaseManifest = { version: 1; databases: Record<string, string>; slots: ResourceSlot[] };
export type RuntimeEnv = BetaEnvironment & {
  ERPX_ENVIRONMENT: string;
  ERPX_PUBLIC_ORIGIN: string;
  ERPX_MARKETING_ORIGIN?: string;
  ERPX_FRONT_PMS_ORIGIN?: string;
  ERPX_FRONT_POS_ORIGIN?: string;
  ERPX_DATA_DIR: string;
  BETTER_AUTH_URL: string;
  BETTER_AUTH_SECRET: string;
  CONTROL_DB: SqlDatabase;
  CONTROL_READ_DB: SqlDatabase;
  H001_DB: SqlDatabase; H001_READ_DB: SqlDatabase; H001_FILES: FileStore;
  H002_DB: SqlDatabase; H002_READ_DB: SqlDatabase; H002_FILES: FileStore;
  H003_DB: SqlDatabase; H003_READ_DB: SqlDatabase; H003_FILES: FileStore;
  [key: string]: unknown;
};
const manifestSchema = z.object({ version: z.literal(1), databases: z.record(z.string(), z.string().url()),
  slots: z.array(z.object({ id: z.string().regex(/^[a-z0-9]{3,24}$/), write: z.string(), read: z.string(), files: z.string(), beta: z.boolean().optional(), internal: z.boolean().optional() })).max(22) });
const runtimeGlobal = globalThis as typeof globalThis & { erpxDevRuntime?: { key: string; env: RuntimeEnv } };
let singleton: RuntimeEnv | undefined;
function runtimeKey() { return JSON.stringify([process.env.ERPX_DATABASE_MANIFEST, process.env.ERPX_PROCESS_ROLE, process.env.ERPX_DEV_PG_POOL_MAX,process.env.ERPX_APP_PG_POOL_MAX,process.env.ERPX_CONTROL_PG_POOL_MAX, process.env.ERPX_PUBLIC_ORIGIN, process.env.ERPX_MARKETING_ORIGIN, process.env.ERPX_FRONT_PMS_ORIGIN, process.env.ERPX_FRONT_POS_ORIGIN, process.env.ERPX_DATA_DIR, process.env.ERPX_MINIO_ENDPOINT, process.env.ERPX_MINIO_BUCKET]); }

export function getRuntime(): RuntimeEnv {
  if (singleton) return singleton;
  if (process.env.NODE_ENV === 'development' && runtimeGlobal.erpxDevRuntime?.key === runtimeKey()) return singleton = runtimeGlobal.erpxDevRuntime.env;
  if (process.env.NODE_ENV === 'development' && runtimeGlobal.erpxDevRuntime) {
    void closeDatabases(runtimeGlobal.erpxDevRuntime.env).catch(() => {});
    delete runtimeGlobal.erpxDevRuntime;
  }
  const production = process.env.NODE_ENV === 'production';
  const engineering = production ? engineeringVerificationContext() : null;
  const data = resolve(/* turbopackIgnore: true */ process.env.ERPX_DATA_DIR ?? '.runtime/local');
  if (production && !isAbsolute(process.env.ERPX_DATA_DIR ?? '')) throw new Error('ERPX_DATA_DIR_REQUIRED');
  // Parse before constructing any pool. A valid attestation can relax HTTPS
  // only for the exact disposable database manifest and loopback 3104 hosts.
  const manifestPath = process.env.ERPX_DATABASE_MANIFEST;
  if (!manifestPath || !isAbsolute(manifestPath)) throw new Error('ERPX_DATABASE_MANIFEST_REQUIRED');
  const manifest = manifestSchema.parse(JSON.parse(readFileSync(manifestPath, 'utf8')));
  if(engineering){
    if(resolve(manifestPath)!==join(data,'databases.json')||!Object.entries(manifest.databases).every(([binding,value])=>{
      if(!/^(CONTROL|H00[1-3])(_READ)?_DB$/.test(binding))return false;
      const url=new URL(value);
      return ['postgres:','postgresql:'].includes(url.protocol)&&url.hostname==='127.0.0.1'&&url.port==='5432'&&url.username==='erpx_test_admin'&&url.pathname===`/erpx_browser_${engineering.run}_${binding.toLowerCase()}`;
    })||Object.keys(manifest.databases).length!==8||!['CONTROL_DB','CONTROL_READ_DB','H001_DB','H001_READ_DB','H002_DB','H002_READ_DB','H003_DB','H003_READ_DB'].every(binding=>Boolean(manifest.databases[binding]))||manifest.slots.length!==3||new Set(manifest.slots.map(slot=>slot.id)).size!==3||manifest.slots.some(slot=>!/^h00[1-3]$/.test(slot.id)||slot.write!==`${slot.id.toUpperCase()}_DB`||slot.read!==`${slot.id.toUpperCase()}_READ_DB`||slot.files!==`${slot.id.toUpperCase()}_FILES`))throw new Error('ENGINEERING_DATABASE_MANIFEST_MISMATCH');
    if(process.env.ERPX_MINIO_ENDPOINT||process.env.ERPX_MINIO_BUCKET)throw new Error('ENGINEERING_LOCAL_FILES_REQUIRED');
  }
  const deploymentEnvironment = engineering?'local':production && process.env.ERPX_ENVIRONMENT !== 'staging'
    ? 'production'
    : process.env.ERPX_ENVIRONMENT ?? (production ? 'production' : 'local');
  const engineeringOrigin=(url:URL)=>isEngineeringVerificationOrigin(url,engineering);
  const origin = new URL(process.env.ERPX_PUBLIC_ORIGIN ?? 'http://localhost:3100');
  if (origin.pathname !== '/' || origin.search || origin.hash || origin.username || origin.password || (production && origin.protocol !== 'https:'&&!engineeringOrigin(origin))||(engineering&&!engineeringOrigin(origin))) throw new Error('INVALID_PUBLIC_ORIGIN');
  const marketing = process.env.ERPX_MARKETING_ORIGIN ? new URL(process.env.ERPX_MARKETING_ORIGIN) : null;
  if (marketing && (marketing.pathname !== '/' || marketing.search || marketing.hash || marketing.username || marketing.password || (production && marketing.protocol !== 'https:'&&!engineeringOrigin(marketing))||(engineering&&!engineeringOrigin(marketing)) || marketing.host === origin.host)) throw new Error('INVALID_MARKETING_ORIGIN');
  const frontOrigins = ['ERPX_FRONT_PMS_ORIGIN','ERPX_FRONT_POS_ORIGIN'].flatMap((key) => {
    const value=process.env[key];
    if(!value)return [];
    const url=new URL(value);
    if(url.pathname!=='/'||url.search||url.hash||url.username||url.password||(production&&url.protocol!=='https:'&&!engineeringOrigin(url))||(engineering&&!engineeringOrigin(url)))throw new Error('INVALID_FRONT_ORIGIN');
    return [{key,url}];
  });
  if(new Set([origin.host,...(marketing ? [marketing.host] : []),...frontOrigins.map(({url})=>url.host)]).size!==frontOrigins.length+1+(marketing ? 1 : 0))throw new Error('DUPLICATE_PUBLIC_HOST');
  const secret = process.env.BETTER_AUTH_SECRET ?? '';
  if (secret.length < 32 || /^(generate-|local-development)/.test(secret)) throw new Error('BETTER_AUTH_SECRET_REQUIRED');
  // The manifest is installed outside the release. A mandatory environment path
  // also prevents Next's file tracer from including the development workspace.
  const publicBeta = process.env.ERPX_PUBLIC_BETA === 'true';
  if (production && publicBeta && (!process.env.RESEND_API_KEY || !process.env.RESEND_FROM)) throw new Error('REGISTRATION_EMAIL_NOT_CONFIGURED');
  const env: Record<string, unknown> = {
    ERPX_ENVIRONMENT: deploymentEnvironment,
    ERPX_PUBLIC_ORIGIN: origin.origin, ERPX_DATA_DIR: data,
    BETTER_AUTH_URL: origin.origin, BETTER_AUTH_SECRET: secret,
    ERPX_RESOURCE_SLOTS: JSON.stringify(manifest.slots), ERPX_PUBLIC_BETA: String(publicBeta),
  };
  if (marketing) env.ERPX_MARKETING_ORIGIN = marketing.origin;
  for(const {key,url} of frontOrigins)env[key]=url.origin;
  for (const name of ['BETA_MEMBER_LIMIT','BETA_BRANCH_LIMIT','BETA_AI_LIMIT','BETA_STORAGE_BYTES','RESEND_API_KEY','RESEND_FROM','ERPX_EMAIL_ADAPTER',
    'ERPX_IAM_SECRET_PROFILES','AI_BASE_URL','AI_MODEL','AI_API_KEY','AI_CREDENTIAL_KEY_ID','AI_CREDENTIAL_KEYS','STRIPE_SECRET_KEY','STRIPE_WEBHOOK_SECRET','STRIPE_PRICE_ID'])
    if (process.env[name]) env[name] = process.env[name];
  if (process.env.ERPX_MINIO_ENDPOINT || process.env.ERPX_MINIO_BUCKET) {
    if (!process.env.ERPX_MINIO_ENDPOINT || !process.env.ERPX_MINIO_BUCKET || !process.env.ERPX_MINIO_ACCESS_KEY || !process.env.ERPX_MINIO_SECRET_KEY) throw new Error('ERPX_MINIO_CONFIG_INCOMPLETE');
    const endpoint = new URL(process.env.ERPX_MINIO_ENDPOINT);
    if (!['http:', 'https:'].includes(endpoint.protocol) || endpoint.pathname !== '/' || endpoint.search || endpoint.hash) throw new Error('ERPX_MINIO_ENDPOINT_INVALID');
  }
  const role = process.env.ERPX_PROCESS_ROLE === 'jobs' ? 'jobs' : 'app';
  const poolLimits=databasePoolLimits({application:process.env.ERPX_APP_PG_POOL_MAX,development:process.env.ERPX_DEV_PG_POOL_MAX,control:process.env.ERPX_CONTROL_PG_POOL_MAX});
  for (const [binding, connectionString] of Object.entries(manifest.databases)) {
    // Installer provides independently scoped database logins; a separate jobs
    // manifest grants projection writes that the web process cannot perform.
    env[binding] = new PostgresDatabase({ connectionString, application_name: `erpx-${role}`, max:databasePoolSize(binding,role,process.env.NODE_ENV,poolLimits) });
  }
  // Schema migrations use a separately provisioned login. The web process gets
  // these credentials only from the private, sibling migration manifest; app
  // and jobs URLs remain unable to migrate schema or read erpx_migrations.
  if (process.env.ERPX_PROCESS_ROLE !== 'jobs') {
    try {
      const migrationManifestPath = process.env.ERPX_MIGRATION_DATABASE_MANIFEST ?? join(dirname(manifestPath), 'databases-migrations.json');
      if (!isAbsolute(migrationManifestPath)) throw new Error('ERPX_MIGRATION_DATABASE_MANIFEST_REQUIRED');
      if(engineering&&resolve(migrationManifestPath)!==join(data,'databases-migrations.json'))throw new Error('ENGINEERING_MIGRATION_MANIFEST_MISMATCH');
      const migrationManifest = manifestSchema.parse(JSON.parse(readFileSync(/* turbopackIgnore: true */ migrationManifestPath, 'utf8')));
      if(engineering&&(Object.keys(migrationManifest.databases).length!==8||Object.entries(manifest.databases).some(([binding,url])=>migrationManifest.databases[binding]!==url)||JSON.stringify(migrationManifest.slots)!==JSON.stringify(manifest.slots)))throw new Error('ENGINEERING_MIGRATION_MANIFEST_MISMATCH');
      for (const [binding, connectionString] of Object.entries(migrationManifest.databases)){
        if(engineering){
          const url=new URL(connectionString);
          if(!/^(CONTROL|H00[1-3])(_READ)?_DB$/.test(binding)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${engineering.run}_${binding.toLowerCase()}`)throw new Error('ENGINEERING_MIGRATION_MANIFEST_MISMATCH');
        }
        env[`MIGRATION_${binding}`] = new PostgresDatabase({ connectionString, application_name: 'erpx-migration', max: 1 });
      }
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    }
  }
  if (!env.CONTROL_DB) throw new Error('CONTROL_DATABASE_REQUIRED');
  for (const slot of manifest.slots) {
    if (!env[slot.write] || !env[slot.read]) throw new Error('HOLDING_DATABASE_REQUIRED');
    const local = new LocalFileStore(join(data, 'files', slot.id), env[slot.write] as PostgresDatabase);
    env[slot.files] = process.env.ERPX_MINIO_ENDPOINT && process.env.ERPX_MINIO_BUCKET
      ? new MinioFileStore(process.env.ERPX_MINIO_BUCKET, `environments/${deploymentEnvironment}/holdings/${slot.id}`, process.env.ERPX_MINIO_ENDPOINT, process.env.ERPX_MINIO_ACCESS_KEY ?? '', process.env.ERPX_MINIO_SECRET_KEY ?? '', process.env.ERPX_MINIO_REGION ?? 'us-east-1', env[slot.write] as PostgresDatabase, local)
      : local;
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
