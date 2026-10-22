import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { crmOpportunityEvidenceRow } from "@erpx/server-crm/public/crm";
import { customerFollowupEvidenceRow } from "@erpx/server-sales/public/customer-followups";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
import { createCrmEvidenceApplication, type CrmEvidenceRepository } from '@erpx/server-runtime/application/executive/crm-evidence';
import { executiveEvidenceSources } from '@erpx/server-crm/public/executive-evidence-sources';
import {executiveFollowupEvidenceSources} from '@erpx/server-sales/public/executive-followup-evidence-sources';
import { BusinessRuleError } from '@erpx/server-shared/index';
function application(database: SqlDatabase) {
    const repository: CrmEvidenceRepository = { ...executiveEvidenceSources(database), ...executiveFollowupEvidenceSources(database), source0(companyId: string, userId: string, assignmentId: string, provider: string, metric: "opportunity-pipeline" | "overdue-followups", action: "search" | "open", value: string, queryHash: string, now: number, value2: string) {
            return database.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
                .bind(companyId, userId, assignmentId, provider, metric, action, value, queryHash, now, value2).run();
        }, source1() { return database.prepare("SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') utc_date").first<{
            utc_date: string;
        }>(); } };
    return createCrmEvidenceApplication({ repository, clock: { now: Date.now }, digest: sha256, assertProductionModuleEnabled: (...args) => fromNative(() => assertProductionModuleEnabled(database, ...args)), crmOpportunityEvidenceRow: (...args) => fromNative(() => crmOpportunityEvidenceRow(...args, database)), customerFollowupEvidenceRow: (...args) => fromNative(() => customerFollowupEvidenceRow(...args, database)), currentAuthority: (...args) => fromNative(() => currentAuthority(database, ...args)), metricCatalogFor: (...args) => fromNative(() => metricCatalogFor(database, ...args)) });
}
async function fromNative<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<T>(run: () => Promise<T> | T): Promise<T> { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function searchCrmEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) { return invoke(() => application(db).searchCrmEvidence(actor, raw)); }
export async function readCrmEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) { return invoke(() => application(db).readCrmEvidence(actor, metric, recordId, rawFilters)); }
export { crmEvidenceMetrics } from '@erpx/server-runtime/domain/executive/crm-evidence';
