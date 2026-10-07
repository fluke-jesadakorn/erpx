import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export async function appExecutiveCacheRevisions(db: SqlDatabase) { return (await db.prepare("SELECT table_name,revision FROM work_cache_revisions ORDER BY table_name").all()).results; }
