import { getRuntime,closeRuntime } from '../../src/server/runtime/env';
import { moduleRegistry } from '../../src/data/module-registry';
import type { SqlDatabase } from '../../src/server/runtime/types';

export async function bootstrapAdmin(db: SqlDatabase,email: string, holdingDb?: SqlDatabase) {
  const allowed=process.env.ERPX_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com';
  if(email.toLowerCase()!==allowed.toLowerCase())throw new Error('ADMIN_EMAIL_NOT_ALLOWED');
  const user=await db.prepare('SELECT id,emailVerified FROM "user" WHERE lower(email)=lower(?)').bind(email).first<{id:string;emailVerified:number}>();
  if(!user || Number(user.emailVerified)!==1)throw new Error('ADMIN_MUST_SIGN_UP_AND_VERIFY_FIRST');
  const now=Date.now();
  await db.batch([
    db.prepare(`INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,version)
      VALUES(?,?,'platform_admin',?,?,?,1) ON CONFLICT(id) DO NOTHING`)
      .bind(`bootstrap-admin-${user.id}`,user.id,JSON.stringify(moduleRegistry.map(m=>m.id)),now,now+365*86400000),
    ...(process.env.ERPX_ENVIRONMENT === 'local' && user.id === 'usr_jesadakorn'
      ? [db.prepare(`INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,version)
          VALUES(?,?,'platform_auditor','[]',?,?,1) ON CONFLICT(id) DO NOTHING`)
          .bind(`erpx-auditor-${user.id}`,user.id,now,now+365*86400000)]
      : []),
  ]);
  if (holdingDb) {
    await holdingDb.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,status,hierarchy_revision) VALUES('erpx-co','company','erpx','ERPX',NULL,'erpx-co','active',1) ON CONFLICT(id) DO NOTHING").run();
    await holdingDb.prepare(`INSERT INTO assignments(id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,version,scope_kind,scope_node_id)
      VALUES(?,?,'holding_owner',?,'erpx-co',NULL,?,?,1,'holding',NULL) ON CONFLICT(id) DO NOTHING`)
      .bind(`erpx-owner-${user.id}`,user.id,JSON.stringify(moduleRegistry.map(m=>m.id)),now,now+365*86400000).run();
  }
  return {email,role:'platform_admin',erpxHoldingOwner:!!holdingDb};
}
if(import.meta.main){try{
  const env=getRuntime();
  const email=process.argv[2]??process.env.ERPX_ADMIN_EMAIL;
  if(!email)throw new Error('ERPX_ADMIN_EMAIL_REQUIRED');
  console.log(await bootstrapAdmin(env.CONTROL_DB,email,env.H003_DB));
}finally{await closeRuntime();}}
