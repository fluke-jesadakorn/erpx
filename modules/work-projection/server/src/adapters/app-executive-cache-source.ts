import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { workCacheRevisionsSql } from "./read-tail-sql";
export async function appExecutiveCacheRevisions(db: SqlDatabase) { return (await db.prepare(workCacheRevisionsSql()).all()).results; }
