import {ppRd36NumberedTwoPred} from './pp-read-tail-r36-sources';
import { nativePpQuery } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import "server-only";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { IamActor } from "@erpx/server-identity_access/public/iam-context";
import { assertCompanyScope } from "@erpx/server-identity_access/public/iam-context";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import { iamCommand } from "@erpx/server-identity_access/public/iam-transactions";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { PoolClient, Pool } from 'pg';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { peopleTemplateAttributes } from '@erpx/server-identity_access/public/people-template-attributes';
import { adoptTemplateInput, type TemplateRow, type SettingsRow } from '@erpx/server-people_payroll/domain/people-templates';
import { createPeopleTemplateApplication, type PeopleTemplateRepository } from '@erpx/server-people_payroll/application/people-templates';
export { adoptTemplateInput } from '@erpx/server-people_payroll/domain/people-templates';
function repository(client: PoolClient | Pool): PeopleTemplateRepository {
    return { templates(companyId: string, holdingId: string | null) {
            return client.query<TemplateRow>("SELECT t.*,a.template_version adopted_version FROM people_holding_templates t LEFT JOIN people_template_adoptions a ON a.template_id=t.id AND a.company_id=$1 WHERE t.holding_id=$2 ORDER BY t.name", [companyId, holdingId]);
        },
        publishedTemplate(id: string, holdingId: string | null) {
            return client.query<TemplateRow>("SELECT * FROM people_holding_templates WHERE id=$1 AND holding_id=$2 FOR UPDATE", [id, holdingId]);
        },
        updateTemplate(name: string, value: string, now: number, id: string) {
            return nativePpQuery<TemplateRow>('pp_people_templates_stmt_1', [name, value, now, id]);
        },
        insertTemplate(id: string, holdingId: string | null, name: string, value: string, userId: string, now: number) {
            return nativePpQuery<TemplateRow>('pp_people_templates_stmt_2', [
                id,
                holdingId,
                name,
                value,
                userId,
                now,
            ]);
        },
        adoptionTemplate(templateId: string, holdingId: string | null) {
            return client.query<TemplateRow>(ppRd36NumberedTwoPred("*", "people_holding_templates", "id", "holding_id"), [templateId, holdingId]);
        },
        settings(companyId: string) {
            return client.query<SettingsRow>("SELECT * FROM people_lifecycle_settings WHERE company_id=$1 FOR UPDATE", [companyId]);
        },
        attributes(...args) { return peopleTemplateAttributes(client as PoolClient).attributes(...args); },
        activeRole(...args) { return peopleTemplateAttributes(client as PoolClient).activeRole(...args); },
        groupReferences(...args) { return peopleTemplateAttributes(client as PoolClient).groupReferences(...args); },
        upsertAttribute(...args) { return peopleTemplateAttributes(client as PoolClient).upsertAttribute(...args); },
        attributeHistory(...args) { return peopleTemplateAttributes(client as PoolClient).attributeHistory(...args); },
        updateSettings(companyId: string, value: string) {
            return nativePpQuery<Record<string, unknown>>('pp_people_templates_stmt_3', [companyId, value]);
        },
        recordAdoption(companyId: string, id: unknown, version: unknown, value: string) {
            return nativePpQuery<Record<string, unknown>>('pp_people_templates_stmt_4', [companyId, id, version, value]);
        } };
}
async function external<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function exposed<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
function application(db: PostgresDatabase) { return createPeopleTemplateApplication({ repository: repository(iamDb(db).pool), clock: { now: Date.now }, identifiers: { next: randomUUID }, hash: value => createHash('sha256').update(value).digest('hex'), companyScope: assertCompanyScope, authorize: requirePermission, command: (actor, key, operation, input, permission, run) => external(() => iamCommand(db, actor, key, operation, input, permission, c => run(repository(c)))), preview: async (run) => { const c = await iamDb(db).pool.connect(); try {
        await c.query('BEGIN ISOLATION LEVEL REPEATABLE READ');
        return await run(repository(c));
    }
    finally {
        await c.query('ROLLBACK');
        c.release();
    } } }); }
export async function peopleTemplatesData(db: PostgresDatabase, actor: IamActor) { return exposed(() => application(db).peopleTemplatesData(actor)); }
export async function saveHoldingTemplate(db: PostgresDatabase, actor: IamActor, raw: unknown, key: string) { return exposed(() => application(db).saveHoldingTemplate(actor, raw, key)); }
export async function adoptHoldingTemplate(db: PostgresDatabase, actor: IamActor, input: z.infer<typeof adoptTemplateInput>, key: string, preview: boolean) { return exposed(() => application(db).adoptHoldingTemplate(actor, input, key, preview)); }
