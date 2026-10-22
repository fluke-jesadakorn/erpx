import "server-only";
import { createHash } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { ApiError } from "@erpx/server-runtime/public/http";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { previewLifecycle, saveLifecycle } from "@erpx/server-people_payroll/adapters/people-lifecycle";
import { executeCommand, replayCommand } from "@erpx/server-workflow/public/commands";
import { isTransactionalSqlDatabase, type SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { peoplePlacementBatchFacts } from '@erpx/server-organization/public/people-placement-batch-facts';
import { type PlacementBatchInput } from '@erpx/server-people_payroll/domain/people-placement-batch';
import { createPeoplePlacementBatchApplication, type PeoplePlacementBatchPorts } from '@erpx/server-people_payroll/application/people-placement-batch';
export { placementBatchInput, type PlacementBatchInput } from '@erpx/server-people_payroll/domain/people-placement-batch';
function context(database: SqlDatabase): PeoplePlacementBatchPorts<SqlStatement> {
    return { repository: { currentEmployees(companyId: string) {
                return database.prepare("SELECT e.id engagement_id,e.person_id,e.status,e.position_id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.status!='ended' AND p.status!='archived' AND p.merged_into_id IS NULL")
                    .bind(companyId).all<{
                    engagement_id: string;
                    person_id: string;
                    status: string;
                    position_id: string | null;
                }>();
            }, activePositions(...args) { return peoplePlacementBatchFacts(database).activePositions(...args); } }, authorize: requirePermission, hash: value => createHash('sha256').update(value).digest('hex'), commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, preview: (...args) => fromNative(() => previewLifecycle(database, ...args)), save: (...args) => fromNative(() => saveLifecycle(database, ...args)), transaction: run => {
            if (!isTransactionalSqlDatabase(database))
                throw new Error('PAYROLL_TRANSACTION_REQUIRED');
            return database.transaction(tx => run(context(tx)));
        } };
}
async function fromNative<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function previewPlacementBatch(db: SqlDatabase, actor: WorkContext, input: PlacementBatchInput) { return invoke(() => createPeoplePlacementBatchApplication(context(db)).previewPlacementBatch(actor, input)); }
export async function submitPlacementBatch(db: PostgresDatabase, actor: WorkContext, input: PlacementBatchInput, token: string, key: string) { return invoke(() => createPeoplePlacementBatchApplication(context(db)).submitPlacementBatch(actor, input, token, key)); }
