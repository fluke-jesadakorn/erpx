import "server-only";
import { z } from "zod";
import { iamOverview } from "@/server/modules/identity_access/public/iam-service";
import type { IamActor } from "@/server/modules/identity_access/public/iam-context";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requireFlowAccess } from "@/server/modules/workflow/public/flow-access";
import { resolveFlow } from "@/server/modules/workflow/public/flow-routing";
import { flowRecord } from "@/server/modules/workflow/public/flow-routing";
import { createIamInspector } from '../application/inspector';
import { inspectorInput } from '../domain/inspector';
export { inspectorInput } from '../domain/inspector';
export async function inspectIamAccess(raw: PostgresDatabase, admin: IamActor, input: z.infer<typeof inspectorInput>) {
    const db = iamDb(raw);
    return createIamInspector({ error: (...args) => new ApiError(...args), apiError: (error) => error instanceof ApiError, overview: (admin) => iamOverview(db, admin), person: async (companyId, userId) => (await db.prepare('SELECT person_id FROM iam_members WHERE company_id=? AND user_id=?').bind(companyId, userId).first<{
            person_id: string | null;
        }>())?.person_id, aliases: async (companyId, person) => (await db.prepare('SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?').bind(companyId, person).all<{
            user_id: string;
        }>()).results, access: requireFlowAccess, record: (companyId, kind, id) => flowRecord(raw, companyId, kind, id), flow: (actor, kind, id) => resolveFlow(raw, actor, kind, id) })(admin, input);
}
