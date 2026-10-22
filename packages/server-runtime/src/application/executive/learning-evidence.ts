import { z } from "zod";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { filtersSchema } from "@erpx/server-runtime/public/rules/canvas-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { learningEvidenceMetrics, type LearningEvidenceMetric, providerId, completionFields, overdueFields, inputSchema, type LearningEvidenceRow, parseCursor, encodeCursor, safeRevision, safeTimestamp, notFound, utcRange } from '@erpx/server-runtime/domain/executive/learning-evidence';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { learningExecutiveEvidenceRow } from "@erpx/server-learning/public/learning";
import type { assertProductionModuleEnabled } from "@erpx/server-pms/public/hospitality-rollout";
import type { currentAuthority } from "@erpx/server-runtime/public/executive/access";
import type { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
export interface LearningEvidenceRepository {
    searchRows0(actor: WorkContext, input: z.infer<typeof inputSchema>, cursor: ReturnType<typeof parseCursor>): Promise<LearningEvidenceRow[]>;
    source0(companyId: string, userId: string, assignmentId: string, providerId: "learning-assignment-records", metric: "learning-completions" | "overdue-required-learning", action: "search" | "open", value: string, queryHash: string, now: number, value2: string): Promise<{
        success: boolean;
        meta?: {
            changes?: number;
            duration?: number;
            last_row_id?: number | string;
        };
    }>;
}
export interface LearningEvidencePorts {
    repository: LearningEvidenceRepository;
    clock: Clock;
    digest(value: string): Promise<string>;
    learningExecutiveEvidenceRow(...args: Tail<Parameters<typeof learningExecutiveEvidenceRow>>): ReturnType<typeof learningExecutiveEvidenceRow>;
    assertProductionModuleEnabled(...args: Tail<Parameters<typeof assertProductionModuleEnabled>>): ReturnType<typeof assertProductionModuleEnabled>;
    currentAuthority(...args: Tail<Parameters<typeof currentAuthority>>): ReturnType<typeof currentAuthority>;
    metricCatalogFor(...args: Tail<Parameters<typeof metricCatalogFor>>): ReturnType<typeof metricCatalogFor>;
}
export function createLearningEvidenceApplication(ports: LearningEvidencePorts) {
    const r = ports.repository;
    async function requireLearningEvidence(actor: WorkContext, metric: LearningEvidenceMetric) {
        await ports.currentAuthority(actor);
        if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "learning"))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        const catalog = await ports.metricCatalogFor(actor);
        if (!catalog.some((item) => item.id === metric && item.available))
            throw new BusinessRuleError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
        await ports.assertProductionModuleEnabled(actor.companyId, "learning");
    }
    async function writeAudit(actor: WorkContext, metric: LearningEvidenceMetric, action: "search" | "open", rows: {
        id: string;
        version: number;
    }[], queryHash: string) {
        if (!rows.length)
            return;
        const fields = metric === "learning-completions" ? completionFields : overdueFields;
        await r.source0(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fields), queryHash, ports.clock.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))));
    }
    async function searchLearningEvidence(actor: WorkContext, raw: unknown) {
        const input = inputSchema.parse(raw);
        await requireLearningEvidence(actor, input.metric);
        const cursor = parseCursor(input.cursor, input.metric);
        const rows = await r.searchRows0(actor, input, cursor);
        const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
        const queryHash = await ports.digest(JSON.stringify({
            providerId, metric: input.metric, filters: input.filters, query: input.query,
            cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, input.metric, "search", page.map((row) => ({
            id: row.id, version: safeRevision(input.metric === "learning-completions" ? row.event_version : row.version),
        })), queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: input.metric,
            fields: input.metric === "learning-completions" ? completionFields : overdueFields,
            items: page.map((row) => {
                const completion = input.metric === "learning-completions";
                const updatedAt = completion ? safeTimestamp(row.created_at) : safeTimestamp(row.updated_at);
                const assignmentId = completion ? row.assignment_id! : row.id;
                return {
                    id: row.id,
                    label: row.course_title,
                    summary: completion
                        ? `Completed · ${new Date(updatedAt).toISOString().slice(0, 10)}`
                        : `${row.status.replaceAll("_", " ")} · due ${row.due_date}`,
                    status: completion ? "completed" : row.status,
                    version: safeRevision(completion ? row.event_version : row.version),
                    href: `/learning/${encodeURIComponent(assignmentId)}`,
                };
            }),
            hasMore,
            nextCursor: hasMore && page.length ? encodeCursor(input.metric, page.at(-1)!) : null,
            queryHash,
        };
    }
    async function readLearningEvidence(actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) {
        const parsedMetric = z.enum(learningEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
        await requireLearningEvidence(actor, parsedMetric);
        const range = parsedMetric === "learning-completions" ? utcRange(filters) : undefined;
        const row = await ports.learningExecutiveEvidenceRow(actor, parsedMetric, recordId, range) as LearningEvidenceRow | null;
        if (!row)
            throw notFound();
        const completion = parsedMetric === "learning-completions";
        const version = safeRevision(completion ? row.event_version : row.version);
        const record = {
            id: row.id,
            assignment_id: completion ? row.assignment_id! : row.id,
            course_title: row.course_title,
            status: row.status,
            created_at: completion ? safeTimestamp(row.created_at) : null,
            due_date: completion ? null : row.due_date!,
            required_for: completion ? null : row.required_for!,
            version,
            href: `/learning/${encodeURIComponent(completion ? row.assignment_id! : row.id)}`,
        };
        const queryHash = await ports.digest(JSON.stringify({
            providerId, metric: parsedMetric, filters,
            scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId,
        }));
        await ports.currentAuthority(actor);
        await writeAudit(actor, parsedMetric, "open", [{ id: row.id, version }], queryHash);
        await ports.currentAuthority(actor);
        return {
            provider: providerId,
            metric: parsedMetric,
            fields: completion ? completionFields : overdueFields,
            record,
            queryHash,
        };
    }
    return { searchLearningEvidence, readLearningEvidence };
}
