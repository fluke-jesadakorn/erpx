import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function appUploadTaskState(db: SqlDatabase, id: string, companyId: string) { return db.prepare("SELECT 1").bind().first<{ state: string }>(); }
