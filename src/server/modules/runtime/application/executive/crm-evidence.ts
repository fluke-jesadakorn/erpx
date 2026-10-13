import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/identity_access/permissions";
import { filtersSchema, type Filters } from "@/server/modules/runtime/public/rules/canvas-domain";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { crmEvidenceMetrics, type CrmEvidenceMetric, opportunityProviderId, opportunityMetricId, followupProviderId, followupMetricId, openStages, fields, followupFields, inputSchema, type OpportunityRow, type FollowupRow, parseCursor, encodeCursor, parseFollowupCursor, encodeFollowupCursor, exactVersion, exactMinor, notFound, auditRecords } from '../../domain/executive/crm-evidence';
import { BusinessRuleError, type Clock } from '../../../shared';
import type { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { crmOpportunityEvidenceRow } from "@/server/modules/crm/public/crm";
import type { customerFollowupEvidenceRow } from "@/server/modules/sales/public/customer-followups";
import type { currentAuthority } from "@/server/modules/runtime/public/executive/access";
import type { metricCatalogFor } from "@/server/modules/runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface CrmEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<OpportunityRow[]>;
    searchRows1(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseFollowupCursor>, referenceDate: string): Promise<FollowupRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, provider: string, metric: "opportunity-pipeline" | "overdue-followups", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
    source1(): Promise<{
        utc_date: string;
    } | null>;
}
export interface CrmEvidencePorts {
    repository: CrmEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    crmOpportunityEvidenceRow(actor: WorkContext, id: string): ReturnType<typeof crmOpportunityEvidenceRow>;
    customerFollowupEvidenceRow(actor: WorkContext, id: string, from: string, to: string, beforeDate: string): ReturnType<typeof customerFollowupEvidenceRow>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createCrmEvidenceApplication(ports: CrmEvidencePorts) {
    const r = ports.repository;
    async function requireCrmEvidence(actor: WorkContext, metric: CrmEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "commerce.read") || !hasModuleAccess(actor.modules, "crm"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "crm");
    }
    async function writeAudit(actor: WorkContext, provider: string, metric: CrmEvidenceMetric, action: "search" | "open", records: {
        id: string;
        version: number;
    }[], allowedFields: string[], queryHash: string) {
        if (!records.length)
            return;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, provider, metric, action, JSON.stringify(allowedFields), queryHash, ports.clock.now(), JSON.stringify(records.map(({ id, version }) => ({ record_id: id, record_version: version }))));
    }
    async function searchCrmEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requireCrmEvidence(actor, input.metric);
        if (input.metric === followupMetricId)
            return searchCustomerFollowupEvidence(actor, input);
        const cursor = parseCursor(input.cursor);
        const rows = await r.searchRows0(actor, input, cursor);
        const hasMore = rows.length > input.limit;
        const page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({
            providerId: opportunityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null,
            scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, opportunityProviderId, opportunityMetricId, "search", auditRecords(page), fields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: opportunityProviderId,
            metric: input.metric,
            fields,
            items: page.map((row) => ({
                id: row.id,
                label: row.title,
                summary: `${row.stage} · closes ${row.close_date}`,
                amount: exactMinor(row.expected_minor),
                currency: row.currency,
                status: row.stage,
                version: exactVersion(row.version, "opportunity version"),
                href: `/crm/${encodeURIComponent(row.lead_id)}`,
            })),
            hasMore,
            nextCursor: hasMore && page.length ? encodeCursor(page.at(-1)!) : null,
            queryHash,
        };
    }
    async function readCrmEvidence(actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) {
        const parsedMetric = z.enum(crmEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
        await requireCrmEvidence(actor, parsedMetric);
        if (parsedMetric === followupMetricId)
            return readCustomerFollowupEvidence(actor, recordId, filters);
        const id = z.string().min(1).max(200).parse(recordId);
        // Recheck through a narrow CRM source reader so branch, department and commerce.read stay aligned with the CRM UI.
        const current = await ports.crmOpportunityEvidenceRow(actor, id) as Record<string, unknown> | null;
        if (!current || current.id !== id || !openStages.includes(String(current.stage) as typeof openStages[number])
            || current.currency !== filters.currency || typeof current.close_date !== "string"
            || current.close_date < filters.from || current.close_date > filters.to)
            throw notFound();
        const opportunityVersion = exactVersion(current.opportunity_version as number | string | bigint, "opportunity version");
        const leadVersion = exactVersion(current.lead_version as number | string | bigint, "lead version");
        const record = {
            id,
            lead_id: String(current.lead_id),
            title: String(current.title),
            lead_status: String(current.lead_status),
            lead_version: leadVersion,
            stage: String(current.stage),
            expected_minor: exactMinor(current.expected_minor as number | string | bigint),
            currency: String(current.currency),
            close_date: String(current.close_date),
            version: opportunityVersion,
            href: `/crm/${encodeURIComponent(String(current.lead_id))}`,
        };
        const queryHash = await ports.digest(JSON.stringify({
            providerId: opportunityProviderId, metric: parsedMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId: id,
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, opportunityProviderId, opportunityMetricId, "open", [{ id, version: opportunityVersion }, { id: String(current.lead_id), version: leadVersion }], fields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: opportunityProviderId, metric: parsedMetric, fields, record, queryHash };
    }
    async function currentUtcDate() {
        const row = await r.source1();
        if (!row || !/^\d{4}-\d{2}-\d{2}$/.test(row.utc_date))
            throw new BusinessRuleError(500, "EVIDENCE_REFERENCE_INVALID", "The source did not return a valid UTC date.");
        return row.utc_date;
    }
    async function searchCustomerFollowupEvidence(actor: WorkContext, input: z.infer<typeof inputSchema>) {
        const referenceDate = await currentUtcDate(), cursor = parseFollowupCursor(input.cursor);
        const rows = await r.searchRows1(actor, input, cursor, referenceDate);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({ providerId: followupProviderId, metric: followupMetricId, filters: input.filters, query: input.query, cursor: input.cursor ?? null, referenceDate, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId] }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, followupProviderId, followupMetricId, "search", page.map(row => ({ id: row.id, version: exactVersion(row.version, "follow-up version") })), followupFields, queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: followupProviderId, metric: followupMetricId, fields: followupFields,
            items: page.map(row => ({ id: row.id, label: row.next_action, summary: `Open · due ${row.due_date}`, status: row.status, version: exactVersion(row.version, "follow-up version"), href: `/customer-followups/${encodeURIComponent(row.id)}` })),
            hasMore, nextCursor: hasMore && page.length ? encodeFollowupCursor(page.at(-1)!) : null, queryHash,
        };
    }
    async function readCustomerFollowupEvidence(actor: WorkContext, recordId: string, filters: Filters) {
        const id = z.string().min(1).max(200).parse(recordId), referenceDate = await currentUtcDate();
        const row = await ports.customerFollowupEvidenceRow(actor, id, filters.from, filters.to, referenceDate) as Record<string, unknown> | null;
        if (!row)
            throw notFound();
        const version = exactVersion(row.version as number | string | bigint, "follow-up version");
        const record = {
            id: String(row.id), next_action: String(row.next_action), status: String(row.status), due_date: String(row.due_date), version,
            order_id: String(row.order_id), order_number: String(row.order_number), href: `/customer-followups/${encodeURIComponent(String(row.id))}`,
            order_href: `/commerce/orders/${encodeURIComponent(String(row.order_id))}`,
        };
        const queryHash = await ports.digest(JSON.stringify({ providerId: followupProviderId, metric: followupMetricId, filters, referenceDate, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId: id }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, followupProviderId, followupMetricId, "open", [{ id, version }], followupFields, queryHash);
        await ports.currentAuthority(actor);
        return { provider: followupProviderId, metric: followupMetricId, fields: followupFields, record, queryHash };
    }
    return { searchCrmEvidence, readCrmEvidence };
}
