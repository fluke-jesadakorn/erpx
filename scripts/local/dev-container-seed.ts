/** Seed only through the existing owner scripts; page navigation is never a CLI operation. */
import {mock} from 'bun:test';
mock.module('next/navigation',()=>({
 forbidden:()=>{throw Error('CLI_PAGE_FORBIDDEN');},
 notFound:()=>{throw Error('CLI_PAGE_NOT_FOUND');},
 redirect:()=>{throw Error('CLI_REDIRECT_FORBIDDEN');},
}));
const steps={
 local:()=>import('../seed/seed-local'),
 identity:()=>import('../seed/repair-seed-identity'),
 commerce:()=>import('../seed/seed-commerce'),
 staff:()=>import('../seed/seed-staff-work'),
 'contributor-finance':()=>import('../seed/contributor-finance-demo'),
 'contributor-service':()=>import('../seed/contributor-service-demo'),
 verify:()=>import('../seed/verify-seed-local'),
} as const;
const name=process.argv[2];if(process.argv.length!==3||!name||!Object.hasOwn(steps,name))throw Error('EXACT_LOCAL_SEED_STEP_REQUIRED');
if(name==='verify'){
 // The retained verifier's only SQLite-specific operation is its FK check.
 // Substitute an actual PostgreSQL row audit, leaving every other check intact.
 const fixtures=await import('./fixtures'),{Pool}=await import('pg'),{readFileSync}=await import('node:fs'),{auditDevContainerForeignKeys}=await import('./dev-container-verify-fk');
 const originalGetLocalProxy=fixtures.getLocalProxy,originalExports={...fixtures};
 mock.module(new URL('./fixtures.ts',import.meta.url).pathname,()=>({...originalExports,getLocalProxy:async(...args:Parameters<typeof fixtures.getLocalProxy>)=>{
  const platform=await originalGetLocalProxy(...args),manifest=JSON.parse(readFileSync(process.env.ERPX_DATABASE_MANIFEST!,'utf8')) as {databases:Record<string,string>};
  function wrapped(db:typeof platform.env.H001_DB,binding:string){return new Proxy(db,{get(target,property){if(property==='prepare')return (sql:string)=>{if(sql!=='PRAGMA foreign_key_check')return target.prepare(sql);return {all:async()=>{const pool=new Pool({connectionString:manifest.databases[binding],max:1}),client=await pool.connect();try{await client.query('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY');const results=await auditDevContainerForeignKeys(client);await client.query('COMMIT');return {results};}catch{await client.query('ROLLBACK');throw Error('DEV_CONTAINER_FOREIGN_KEY_AUDIT_FAILED');}finally{client.release();await pool.end();}}};};return Reflect.get(target,property,target);}});}
  return {...platform,env:{...platform.env,H001_DB:wrapped(platform.env.H001_DB,'H001_DB'),H002_DB:wrapped(platform.env.H002_DB,'H002_DB')}};
 }}));
}
await steps[name as keyof typeof steps]();
