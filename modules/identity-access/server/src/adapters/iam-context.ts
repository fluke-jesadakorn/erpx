import 'server-only';
import { currentSession, requireManageErpxContext, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../../platform/runtime/server/src/public/runtime";
import { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { loadIamAuthority } from "@erpx/identity-access-server/public/iam-store";
import { createIamContext, createAssertCompanyScope, createIamActorFromWork } from "@erpx/identity-access-server/application/iam-context";
export type { IamActor } from "@erpx/identity-access-server/domain/iam-actor";
export { iamPermission } from "@erpx/identity-access-server/domain/iam-actor";
const error = (status: number, code: string, message: string) => new ApiError(status, code, message);
export const assertCompanyScope = createAssertCompanyScope({ error });
export const iamActorFromWork = createIamActorFromWork({ error });
export async function iamContext(holdingSlug: string | null) { let db: PostgresDatabase; const actor = await createIamContext({ session: currentSession, work: requireWorkContext, platform: async () => (await requireManageErpxContext()).platform, prepareCompany: async (holdingId) => { db = (await holdingResources(holdingId)).write as PostgresDatabase; }, preparePlatform: async () => { db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase; }, platformAuthority: (userId, role, sessionId) => loadIamAuthority(db, 'platform', userId, role, sessionId) })(holdingSlug); return { db: db!, actor }; }
