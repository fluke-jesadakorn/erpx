import 'server-only';
import { currentSession, requireManageErpxContext, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { ApiError } from "@erpx/server-runtime/public/http";
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import { createIamContext, createAssertCompanyScope, createIamActorFromWork } from "@erpx/server-identity_access/application/iam-context";
export type { IamActor } from "@erpx/server-identity_access/domain/iam-actor";
export { iamPermission } from "@erpx/server-identity_access/domain/iam-actor";
const error = (status: number, code: string, message: string) => new ApiError(status, code, message);
export const assertCompanyScope = createAssertCompanyScope({ error });
export const iamActorFromWork = createIamActorFromWork({ error });
export async function iamContext(holdingSlug: string | null) { let db: PostgresDatabase; const actor = await createIamContext({ session: currentSession, work: requireWorkContext, platform: async () => (await requireManageErpxContext()).platform, prepareCompany: async (holdingId) => { db = (await holdingResources(holdingId)).write as PostgresDatabase; }, preparePlatform: async () => { db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase; }, platformAuthority: (userId, role, sessionId) => loadIamAuthority(db, 'platform', userId, role, sessionId) })(holdingSlug); return { db: db!, actor }; }
