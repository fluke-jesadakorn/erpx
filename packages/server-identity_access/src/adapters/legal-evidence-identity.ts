import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export async function legalEvidenceSamePerson(db: SqlDatabase, companyId: string, actorId: string, authorId: string) { return Boolean(await db.prepare("SELECT 1 WHERE iam_same_person(?,?,?)").bind(actorId, authorId, companyId).first()); }
