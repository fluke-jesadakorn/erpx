import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { roleAllowsSql } from "./iam-probe-sql";
export function calendarAccessIdentityAccessFacts(db: SqlDatabase) { return { permission: (actor: WorkContext, permission: string) => db.prepare(roleAllowsSql).bind(actor.role, actor.companyId, permission).first() }; }
