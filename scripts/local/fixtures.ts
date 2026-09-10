import { getRuntime,closeRuntime } from '../../src/server/runtime/env';
import { resourceSlots } from '../../src/lib/holding-registry';
import { dirname,join } from 'node:path';
import type { SqlDatabase } from '../../src/server/runtime/types';

export async function ensureFixtureLevel(db:SqlDatabase,companyId:string){
  const id=`acceptance-level-${companyId}`,now=Date.now();
  await db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,active,created_at,updated_at) VALUES(?,?,'acceptance-member','Acceptance member',100,1,?,?) ON CONFLICT(id) DO NOTHING")
    .bind(id,companyId,now,now).run();
  return id;
}
/** Development fixture tools can never target an admitted public-beta database. */
export async function getLocalProxy(_options?: unknown) {
  void _options;
  let env=getRuntime();
  if(process.env.NODE_ENV==='production' || resourceSlots(env).some(s=>s.beta))throw new Error('DEVELOPMENT_FIXTURES_ONLY');
  // Seed and HTTP acceptance tools also rebuild projections. Only these local
  // fixture processes use the jobs login; the web keeps SELECT-only read models.
  const jobsManifest=join(dirname(process.env.ERPX_DATABASE_MANIFEST!), 'databases-jobs.json');
  if(process.env.ERPX_DATABASE_MANIFEST!==jobsManifest){
    await closeRuntime();
    process.env.ERPX_DATABASE_MANIFEST=jobsManifest;
    process.env.ERPX_PROCESS_ROLE='jobs';
    env=getRuntime();
    if(resourceSlots(env).some(s=>s.beta))throw new Error('DEVELOPMENT_FIXTURES_ONLY');
  }
  return {env,dispose:closeRuntime};
}
