import {spendingTemplateOwner} from "@erpx/server-organization/public/spending-template-owner";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { executeCommand } from "@erpx/server-workflow/public/commands";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { activeSpendingPosition } from '@erpx/server-organization/public/spending-position';
import { createSpendingTemplateApplication, type SpendingTemplateMutation } from '@erpx/server-people_payroll/application/spending-templates';
export { spendingTemplateSchema } from '@erpx/server-people_payroll/domain/spending-templates';
import type { SpendingTemplateInput } from '@erpx/server-people_payroll/domain/spending-templates';
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
