import "server-only";
import { z } from "zod";

import {iamOverview} from '@erpx/identity-access-server/public/iam-service-reads';
import type { IamActor } from "@erpx/identity-access-server/public/iam-context";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import { iamDb } from "@erpx/identity-access-server/public/iam-db";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { requireFlowAccess } from "@erpx/workflow-server/public/flow-access";
import {resolveFlow} from '@erpx/workflow-server/public/flow-routing';
import {flowRecord} from '@erpx/workflow-server/public/flow-record';
import { createIamInspector } from '@erpx/identity-access-server/application/inspector';
import {memberByCompanyUserSql} from './read-tail-family-r27';
import {iamMemberByCompanyPersonSql} from './read-tail-family-r41';
import { inspectorInput } from '@erpx/identity-access-server/domain/inspector';
export { inspectorInput } from '@erpx/identity-access-server/domain/inspector';
export async function inspectIamAccess(raw: PostgresDatabase, admin: IamActor, input: z.infer<typeof inspectorInput>) {
    const db = iamDb(raw);
    return createIamInspector({ error: (...args) => new ApiError(...args), apiError: (error) => error instanceof ApiError, overview: (admin) => iamOverview(db, admin), person: async (companyId, userId) => (await db.prepare(memberByCompanyUserSql('person_id')).bind(companyId, userId).first<{
            person_id: string | null;
        }>())?.person_id, aliases: async (companyId, person) => (await db.prepare(iamMemberByCompanyPersonSql()).bind(companyId, person).all<{
            user_id: string;
        }>()).results, access: requireFlowAccess, record: (companyId, kind, id) => flowRecord(raw, companyId, kind, id), flow: (actor, kind, id) => resolveFlow(raw, actor, kind, id) })(admin, input);
}
