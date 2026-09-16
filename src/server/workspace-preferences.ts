import type { SqlDatabase } from './runtime/types';

export async function readWorkspacePreference(db: SqlDatabase, userId: string) {
  return db.prepare('SELECT workspace_path,platform_assignment_id FROM account_workspace_preferences WHERE user_id=?')
    .bind(userId).first<{workspace_path:string|null;platform_assignment_id:string|null}>();
}
export async function readCompanyPreference(db: SqlDatabase, userId: string, holdingId: string) {
  return db.prepare('SELECT company_id FROM account_company_preferences WHERE user_id=? AND holding_id=?')
    .bind(userId,holdingId).first<{company_id:string}>();
}
export async function rememberWorkspace(db: SqlDatabase, userId: string, path: string, company?: {holdingId:string;companyId:string}) {
  const now=Date.now();
  await db.batch([
    db.prepare(`INSERT INTO account_workspace_preferences(user_id,workspace_path,updated_at) VALUES(?,?,?)
      ON CONFLICT(user_id) DO UPDATE SET workspace_path=excluded.workspace_path,updated_at=excluded.updated_at`).bind(userId,path,now),
    ...(company ? [db.prepare(`INSERT INTO account_company_preferences(user_id,holding_id,company_id,updated_at) VALUES(?,?,?,?)
      ON CONFLICT(user_id,holding_id) DO UPDATE SET company_id=excluded.company_id,updated_at=excluded.updated_at`).bind(userId,company.holdingId,company.companyId,now)] : []),
  ]);
}
export async function rememberPlatformRole(db: SqlDatabase, userId: string, assignmentId: string) {
  await db.prepare(`INSERT INTO account_workspace_preferences(user_id,platform_assignment_id,updated_at) VALUES(?,?,?)
    ON CONFLICT(user_id) DO UPDATE SET platform_assignment_id=excluded.platform_assignment_id,updated_at=excluded.updated_at`)
    .bind(userId,assignmentId,Date.now()).run();
}
