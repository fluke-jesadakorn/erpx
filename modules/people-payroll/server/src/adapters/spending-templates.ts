import {spendingTemplateOwner} from "@erpx/organization-server/public/spending-template-owner";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { executeCommand } from "@erpx/workflow-server/public/commands";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { activeSpendingPosition } from '@erpx/organization-server/public/spending-position';
import { createSpendingTemplateApplication, type SpendingTemplateMutation } from '@erpx/people-payroll-server/application/spending-templates';
export { spendingTemplateSchema } from '@erpx/people-payroll-server/domain/spending-templates';
import type { SpendingTemplateInput } from '@erpx/people-payroll-server/domain/spending-templates';
function application(db: SqlDatabase) {
    return createSpendingTemplateApplication({ clock: { now: Date.now }, authorize: requirePermission, read: company => spendingTemplateOwner(db).read(company), activePosition: (company, id) => activeSpendingPosition(db, company, id), commit: async <Result>(m: SpendingTemplateMutation, result: Result) => {
            const { actor, input, key, version, now } = m;
            return executeCommand(db, actor, key, "spending.template.save", input, actor.companyId, version, result, await Promise.all([
                spendingTemplateOwner(db).guard(key,actor.companyId,input.expectedVersion),
                spendingTemplateOwner(db).publish(actor.companyId,version,JSON.stringify(input.exceptions),input.reason,actor.userId,now),
            ]));
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
export function saveSpendingTemplate(db: SqlDatabase, actor: WorkContext, input: SpendingTemplateInput, key: string) { return invoke(() => application(db).saveSpendingTemplate(actor, input, key)); }
