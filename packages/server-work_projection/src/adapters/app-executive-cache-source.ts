import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { workCacheRevisionsSql } from "./read-tail-sql";
export async function appExecutiveCacheRevisions(db: SqlDatabase) { return (await db.prepare(workCacheRevisionsSql()).all()).results; }
