import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function appUploadTaskState(db: SqlDatabase, id: string, companyId: string) { return db.prepare("SELECT state FROM staff_tasks WHERE id=? AND company_id=?").bind(id, companyId).first<{ state: string }>(); }
