import {spendingTemplateOwner} from "../../organization/public/spending-template-owner";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { executeCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { BusinessRuleError } from '../../shared';
import { activeSpendingPosition } from '../../organization/public/spending-position';
import { createSpendingTemplateApplication, type SpendingTemplateMutation } from '../application/spending-templates';
export { spendingTemplateSchema } from '../domain/spending-templates';
import type { SpendingTemplateInput } from '../domain/spending-templates';
function application(db: SqlDatabase) {
    return createSpendingTemplateApplication({ clock: { now: Date.now }, authorize: requirePermission, read: company => spendingTemplateOwner(db).read(company), activePosition: (company, id) => activeSpendingPosition(db, company, id), commit: async <Result>(m: SpendingTemplateMutation, result: Result) => {
            const { actor, input, key, version, now } = m;
            return executeCommand(db, actor, key, "spending.template.save", input, actor.companyId, version, result, [
                spendingTemplateOwner(db).guard(key,actor.companyId,input.expectedVersion),
                spendingTemplateOwner(db).publish(actor.companyId,version,JSON.stringify(input.exceptions),input.reason,actor.userId,now),
            ]);
        } });
}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export function spendingTemplate(db: SqlDatabase, companyId: string) { return invoke(() => application(db).spendingTemplate(companyId)); }
export function saveSpendingTemplate(db: SqlDatabase, actor: WorkContext, input: SpendingTemplateInput, key: string) { return invoke(() => application(db).saveSpendingTemplate(actor, input, key)); }
