/** Test-only genuine TypeScript generic Count writer. The selected Account
 * projection and disposable DB name arrive on stdin; credentials stay in env. */
import {readFileSync} from 'node:fs';
Bun.plugin({name:'test-server-only',setup(build){build.module('server-only',()=>({exports:{},loader:'object'}));}});
const {PostgresDatabase}=await import('@erpx/server-runtime/adapters/runtime/postgres');
const {inventoryCountApplication,countSchema}=await import('@erpx/server-inventory/adapters/inventory-counts');
const {assignFlowPrimary}=await import('@erpx/server-workflow/public/flow-routing');
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
catch(error){process.stderr.write(`LEGACY_UPDATE_FAILED:${typeof error==='object'&&error!==null&&'code' in error?String(error.code):'UNKNOWN'}\n`);process.exitCode=1;}
finally{await db.pool.end();}
