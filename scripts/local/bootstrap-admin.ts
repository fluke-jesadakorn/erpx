import { getRuntime,closeRuntime } from '../../src/server/runtime/env';
import { moduleRegistry } from '../../src/data/module-registry';
import type { SqlDatabase } from '../../src/server/runtime/types';

export async function bootstrapAdmin(db: SqlDatabase,email: string) {
  const allowed=process.env.ERPX_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com';
  if(email.toLowerCase()!==allowed.toLowerCase())throw new Error('ADMIN_EMAIL_NOT_ALLOWED');
  const user=await db.prepare('SELECT id,emailVerified FROM "user" WHERE lower(email)=lower(?)').bind(email).first<{id:string;emailVerified:number}>();
  if(!user || Number(user.emailVerified)!==1)throw new Error('ADMIN_MUST_SIGN_UP_AND_VERIFY_FIRST');
  const now=Date.now();
  await db.batch([
    db.prepare(`INSERT INTO platform_assignments(id,user_id,role,modules_json,valid_from,valid_until,version)
      VALUES(?,?,'platform_admin',?,?,?,1) ON CONFLICT(id) DO NOTHING`)
      .bind(`bootstrap-admin-${user.id}`,user.id,JSON.stringify(moduleRegistry.map(m=>m.id)),now,now+365*86400000),
  ]);
  return {email,role:'platform_admin'};
}
if(import.meta.main){try{console.log(await bootstrapAdmin(getRuntime().CONTROL_DB,process.argv[2]??'jesadakorn.kirtnu@gmail.com'));}finally{await closeRuntime();}}
