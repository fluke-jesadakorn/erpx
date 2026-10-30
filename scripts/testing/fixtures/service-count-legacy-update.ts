/** Test-only genuine TypeScript generic Count writer. The selected Account
 * projection and disposable DB name arrive on stdin; credentials stay in env. */
import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
// Historical-only oracle: exact pre-fence sources, never loaded by runtime.
const snapshots={
 '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql':['service-initial-physical-stock-count-source-sql.ts.snapshot','packages/server-inventory/src/adapters','5c0052fad851b80fa136ed72d9d844f2b6b426012bbf26a6aeeb3dd84ae6095d'],
 '@erpx/server-inventory/adapters/inventory-counts':['inventory-counts.ts.snapshot','packages/server-inventory/src/adapters','ab76f9ba70df7bd152646f803b2489c840bea3f00914d20e79e2e021951ccc06'],
 '@erpx/server-inventory/adapters/count-purchases':['count-purchases.ts.snapshot','packages/server-inventory/src/adapters','36616c99b0453d1a3bd89916edd1533a05eb3ee8b84246aa97fa16c0d605ead8'],
 '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source':['service-initial-physical-stock-count-source.ts.snapshot','packages/server-inventory/src/adapters','8c8bf0709b1e827e404678fa408f86f4dc83374371bd50d2dc7039ebcc098a98'],
 '@erpx/server-workflow/adapters/flow-routing':['flow-routing.ts.snapshot','packages/server-workflow/src/adapters','f9c8cfdded42dbb8844e9809de09c95e58fbb7e93bae0a7f6ed9141355ab0d6e'],
} as const;
Bun.plugin({name:'test-legacy-count-oracle',setup(build){
 build.module('server-only',()=>({exports:{},loader:'object'}));
 build.onResolve({filter:/^(?:legacy-count-oracle:|@erpx\/(?:server-inventory\/adapters\/(?:inventory-counts|count-purchases|service-initial-physical-stock-count-source|service-initial-physical-stock-count-source-sql)|server-workflow\/adapters\/flow-routing)$)/},args=>({path:args.path.startsWith('legacy-count-oracle:')?(args.path.endsWith('inventory-counts')?'@erpx/server-inventory/adapters/inventory-counts':'@erpx/server-workflow/adapters/flow-routing'):args.path,namespace:'legacy-count-oracle'}));
 build.onResolve({filter:/^\.\.\/\.\.\/\.\.\/\.\.\/packages\/server-runtime\/src\/public\/runtime$/,namespace:'legacy-count-oracle'},()=>({path:resolve('packages/server-runtime/src/public/runtime.ts')}));
 build.onLoad({filter:/.*/,namespace:'legacy-count-oracle'},args=>{
  const name=args.path==='inventory-counts'?'@erpx/server-inventory/adapters/inventory-counts':args.path==='flow-routing'?'@erpx/server-workflow/adapters/flow-routing':args.path;
  const source=snapshots[name as keyof typeof snapshots];if(!source)throw Error('LEGACY_ORACLE_SOURCE_REQUIRED');
  const bytes=readFileSync(resolve('fixtures/inventory/legacy-count-oracle',source[0]));
  if(createHash('sha256').update(bytes).digest('hex')!==source[2])throw Error('LEGACY_ORACLE_SOURCE_CHANGED');
  let contents=bytes.toString('utf8');
  contents=contents.replaceAll('../../../../packages/server-runtime/src/public/runtime',resolve('packages/server-runtime/src/public/runtime.ts'));
  for(const alias of Object.keys(snapshots)) for(const quote of ["'",'"']) contents=contents.replaceAll(`${quote}${alias}${quote}`,`${quote}legacy-count-oracle:${alias}${quote}`);
  contents=contents.replaceAll('@erpx/server-inventory/public/count-purchases','legacy-count-oracle:@erpx/server-inventory/adapters/count-purchases');
  return {contents,loader:'ts',resolveDir:resolve(source[1])};
 });
}});
const {PostgresDatabase}=await import('@erpx/server-runtime/adapters/runtime/postgres');
const inventoryOracle='legacy-count-oracle:inventory-counts';
const {inventoryCountApplication,countSchema}=await import(inventoryOracle);
const flowOracle='legacy-count-oracle:flow-routing';
const {assignFlowPrimary}=await import(flowOracle);
const {z}=await import('zod');
const flowSchema=z.object({stepKey:z.string().min(1).max(100),userId:z.string().min(1).max(180),expectedVersion:z.number().int().nonnegative(),recordVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000),saveDefault:z.boolean().default(false)});
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
const request=JSON.parse(readFileSync(0,'utf8')) as {database:string;actor:WorkContext;id:string;key:string;body:unknown;mode?:"flow"};
// Restore the property order produced by the real WorkContext constructor; JSON
// transport into this fixture uses a sorted Rust map. Workflow fingerprints retain it.
if(request.actor.authority){const {sessionId,revision,assignmentVersion}=request.actor.authority;request.actor.authority={sessionId,revision,assignmentVersion};}
const token=/^erpx_dev_rust_([a-f0-9]{32})$/.exec(request.database)?.[1];if(!token)throw Error('OWNED_TEST_DATABASE_REQUIRED');
const admin=process.env.ERPX_TEST_ADMIN_URL;if(!admin)throw Error('TEST_ADMIN_REQUIRED');
const url=new URL(admin);if(url.protocol!=='postgresql:'&&url.protocol!=='postgres:'||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!=='/postgres')throw Error('LOCAL_TEST_ADMIN_REQUIRED');url.pathname=`/${request.database}`;
const db=new PostgresDatabase({connectionString:url.href,max:2});
try {const lease=await db.prepare("SELECT pg_get_userbyid(datdba)::text owner,shobj_description(oid,'pg_database') lease FROM pg_database WHERE datname=?").bind(request.database).first<{owner:string;lease:string|null}>();if(lease?.owner!=='erpx_test_admin'||lease.lease!==`erpx-rust-test:${token}`)throw Error('OWNED_TEST_LEASE_REQUIRED');const value=request.mode==="flow"?await assignFlowPrimary(db,request.actor,"stock_count",request.id,flowSchema.parse(request.body),request.key):await db.transaction(tx=>inventoryCountApplication(tx).saveCount(request.actor,request.id,countSchema.parse(request.body),request.key));process.stdout.write(JSON.stringify(value));}
catch(error){process.stderr.write(`LEGACY_UPDATE_FAILED:${typeof error==='object'&&error!==null&&'code' in error?String(error.code):error instanceof Error&&/^[A-Z_]+$/.test(error.message)?error.message:'UNKNOWN'}\n`);process.exitCode=1;}
finally{await db.pool.end();}
