import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { roleAllowsSql } from "./iam-probe-sql";
export function calendarAccessIdentityAccessFacts(db: SqlDatabase) { return { permission: (actor: WorkContext, permission: string) => db.prepare(roleAllowsSql).bind(actor.role, actor.companyId, permission).first() }; }
