import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export function calendarAccessIdentityAccessFacts(db: SqlDatabase) { return { permission: (actor: WorkContext, permission: string) => db.prepare("SELECT 1 WHERE iam_role_allows(?,?,?)").bind(actor.role, actor.companyId, permission).first() }; }
