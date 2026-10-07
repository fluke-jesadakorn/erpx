import 'server-only';
import { currentSession, requireManageErpxContext, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { ApiError } from "@/server/modules/runtime/public/http";
import { loadIamAuthority } from "@/server/modules/identity_access/public/iam-store";
import { createIamContext, createAssertCompanyScope, createIamActorFromWork } from "../application/iam-context";
export type { IamActor } from "../domain/iam-actor";
export { iamPermission } from "../domain/iam-actor";
const error = (status: number, code: string, message: string) => new ApiError(status, code, message);
export const assertCompanyScope = createAssertCompanyScope({ error });
export const iamActorFromWork = createIamActorFromWork({ error });
export async function iamContext(holdingSlug: string | null) { let db: PostgresDatabase; const actor = await createIamContext({ session: currentSession, work: requireWorkContext, platform: async () => (await requireManageErpxContext()).platform, prepareCompany: async (holdingId) => { db = (await holdingResources(holdingId)).write as PostgresDatabase; }, preparePlatform: async () => { db = (await runtimeEnv()).CONTROL_DB as PostgresDatabase; }, platformAuthority: (userId, role, sessionId) => loadIamAuthority(db, 'platform', userId, role, sessionId) })(holdingSlug); return { db: db!, actor }; }
