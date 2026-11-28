import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { samePersonSql } from "./iam-probe-sql";
export async function legalEvidenceSamePerson(db: SqlDatabase, companyId: string, actorId: string, authorId: string) { return Boolean(await db.prepare(samePersonSql).bind(actorId, authorId, companyId).first()); }
