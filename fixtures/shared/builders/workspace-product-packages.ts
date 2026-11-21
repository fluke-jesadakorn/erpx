import {createHash} from 'node:crypto';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import type {EngineeringMoneyPeopleBrowser} from '../../finance/builders/engineering-money-people-browser';

/** Fictional visibility for these owned development snapshots only. This is
 * fixture data, never an installed Commercial purchase or acceptance receipt. */
export async function seedWorkspaceProductPackages(input:{run:string;db:PostgresDatabase;cohort:EngineeringMoneyPeopleBrowser;apps:readonly string[]}){
 const url=new URL(String(input.db.pool.options.connectionString));
 if(!/^[a-f0-9]{12}$/.test(input.run)||input.cohort.run!==input.run||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.username!=='erpx_test_admin'||url.pathname!==`/erpx_browser_${input.run}_h001_db`
   ||!Object.keys(input.cohort.companies).length||!input.apps.length||new Set(input.apps).size!==input.apps.length||input.apps.some(id=>!['apps','people','recruit','finance'].includes(id)))throw Error('DISPOSABLE_PRODUCT_WORKSPACE_FIXTURE_REQUIRED');
 const now=Date.now();
 for(const company of Object.values(input.cohort.companies))await input.db.transaction(async tx=>{
  const owned=await tx.prepare("SELECT id FROM organization_nodes WHERE id=? AND company_id=? AND kind='company' AND status='active'").bind(company.companyId,company.companyId).first();
  if(!owned)throw Error('OWNED_WORKSPACE_COMPANY_REQUIRED');
  for(const productId of input.apps){
   const packageId=productId==='apps'?'work':productId;
   const image={companyId:company.companyId,packageId,packageVersion:1,state:'active',startsAt:now-1000,endsAt:null,version:1,
    reason:`Fictional ${packageId} development-smoke visibility.`,evidence:'Disposable fixture only; no installed Commercial purchase or production certification.'};
   const id=`fixture:workspace:${input.run}:${company.companyId}:${packageId}`;
   await tx.prepare('INSERT INTO company_package_grants(company_id,package_id,package_version,state,starts_at,ends_at,version,reason,evidence,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)')
    .bind(image.companyId,image.packageId,image.packageVersion,image.state,image.startsAt,image.endsAt,image.version,image.reason,image.evidence,now).run();
   await tx.prepare('INSERT INTO company_package_grant_events(id,company_id,package_id,actor_id,request_hash,before_json,after_json,created_at) VALUES(?,?,?,?,?,NULL,?::jsonb,?)')
    .bind(id,company.companyId,packageId,`fixture:workspace:${input.run}`,createHash('sha256').update(JSON.stringify([id,image])).digest('hex'),JSON.stringify(image),now).run();
  }
 });
}
