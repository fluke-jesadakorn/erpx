import { type Breakdown, type Filters, type MetricData, type MetricId, type MetricInfo } from "@/server/modules/runtime/public/rules/canvas-domain";
import type { WorkContext } from '../../../identity_access/public/authorization';
import { BusinessRuleError } from '../../../shared';
import { formatMetric } from "@/lib/shared/executive/numbers";
import { operationsMetrics } from '../../domain/executive/operations-metric-ids';
import type { Clock } from '../../../shared';
import { specs, metricCatalog, metricBreakdowns, metricUnavailable as unavailable, exactInteger, sumExact, percentHundredths, moneyRatioMinor } from '../../domain/executive/metrics';
import {metricSourceSelection,type MetricSourceGroup,type MetricSourcePage} from '../../domain/executive/metric-sources';
export type RawMetricRow = {
    label: string;
    unit?: string;
    value?: string | number | bigint;
    numerator?: string | number | bigint;
    denominator?: string | number | bigint;
    total?: string | number | bigint;
    count_value?: string | number | bigint;
    partial_count?: string | number | bigint;
    configured_count?: string | number | bigint;
};
export interface MetricSnapshot {
    raw: RawMetricRow[];
    referenceAtOverride: number | null;
    source(group: MetricSourceGroup, reported?: string): Promise<MetricSourcePage>;
    watermarks(): Promise<{
        table_name: string;
        revision: string;
    }[]>;
}
export interface ExecutiveMetricPorts {
    clock: Clock;
    digest(value: string): Promise<string>;
    production(): boolean;
    companyAuthority(actor: WorkContext): Promise<boolean>;
    enabledModules(actor: WorkContext): Promise<Set<string>>;
    assertModule(actor: WorkContext, module: string): Promise<void>;
    read(actor: WorkContext, id: MetricId, filters: Filters, breakdown: Breakdown): Promise<MetricSnapshot>;
}
export function createExecutiveMetricApplication(ports: ExecutiveMetricPorts) {
    async function metricCatalogFor(a: WorkContext): Promise<MetricInfo[]> {
        let catalog = metricCatalog(a, ports.production());
        if (catalog.some((metric) => (metric.id === "brand-profiles" || metric.id === "brand-profile-updates") && metric.available)
            && !await ports.companyAuthority(a)) {
            catalog = catalog.map((metric) => metric.id === "brand-profiles" || metric.id === "brand-profile-updates"
                ? { ...metric, available: false, reason: "Company organization-management authority is required for Brand profile records." }
                : metric);
        }
        if (!ports.production())
            return catalog;
        const enabled = await ports.enabledModules(a);
        return catalog.map((metric) => {
            const moduleId = specs[metric.id].moduleId;
            if (metric.acceptancePending)
                return { ...metric, available: false, reason: "Source reconciliation and business acceptance are still pending." };
            if (enabled.has(moduleId))
                return metric;
            return { ...metric, available: false, reason: "This module is outside the approved production pilot or is not enabled for this company." };
        });
    }
    async function queryMetric(a: WorkContext, id: MetricId, f: Filters, breakdown: Breakdown = 'default', sourceGroup?: MetricSourceGroup): Promise<MetricData | MetricSourcePage> {
        const spec = specs[id];
        if (!metricBreakdowns(id).includes(breakdown))
            throw new BusinessRuleError(422, "BREAKDOWN_UNAVAILABLE", "This metric does not support that grouping.");
        if (!(await metricCatalogFor(a)).find((metric) => metric.id === id)?.available)
            throw new BusinessRuleError(403, "METRIC_FORBIDDEN", "This metric is unavailable in the current source permissions, acceptance status or scope.");
        await ports.assertModule(a, spec.moduleId);
        const snapshot = await ports.read(a, id, f, breakdown);
        const { raw, referenceAtOverride } = snapshot;
        let metricTotalOverride: string | null = null, metricPartialOverride = false, metricEmptyOverride = false;
        if (id === "exception-resolution-time") {
            metricTotalOverride = raw[0]?.total === undefined ? "0" : String(exactInteger(raw[0].total));
        }
        if (id === "hotel-occupancy" || id === "hotel-adr" || id === "hotel-revpar")
            metricPartialOverride = raw.some((row) => exactInteger(row.partial_count ?? "0") > BigInt(0));
        if (id === "inventory-shortages")
            metricEmptyOverride = raw.reduce((total, row) => total + exactInteger(row.configured_count ?? "0"), BigInt(0)) === BigInt(0);
        if (id === "overdue-project-milestones")
            metricEmptyOverride = exactInteger(raw[0]?.value ?? "0") === BigInt(0);
        if (id === "project-dependency-blockers")
            metricEmptyOverride = exactInteger(raw[0]?.value ?? "0") === BigInt(0);
        if (id === "project-actual-hours" || id === "project-posted-cost")
            metricEmptyOverride = raw.reduce((total, row) => total + exactInteger(row.count_value ?? "0"), BigInt(0)) === BigInt(0);
        const roomRateMetric = id === "hotel-adr" || id === "hotel-revpar";
        type ExactMetricRow = {
            label: string;
            value: string;
            unit?: string;
            numerator?: string;
            denominator?: string;
        };
        const valueRows = (id === "exception-resolution-time" && exactInteger(raw[0]?.count_value ?? "0") === BigInt(0))
            || (id === "overdue-project-milestones" && metricEmptyOverride)
            || (id === "project-dependency-blockers" && metricEmptyOverride)
            || ((id === "project-actual-hours" || id === "project-posted-cost") && metricEmptyOverride)
            ? []
            : roomRateMetric
                ? raw.filter((row) => exactInteger(row.denominator ?? "0") > BigInt(0))
                : raw;
        const exactRows: ExactMetricRow[] = valueRows.map((row) => spec.unit === "percent"
            ? { label: row.label, value: percentHundredths(row.numerator ?? "0", row.denominator ?? "0"), numerator: String(exactInteger(row.numerator ?? "0")), denominator: String(exactInteger(row.denominator ?? "0")) }
            : roomRateMetric
                ? { label: row.label, value: moneyRatioMinor(row.numerator ?? "0", row.denominator ?? "0"), numerator: String(exactInteger(row.numerator ?? "0")), denominator: String(exactInteger(row.denominator ?? "0")) }
                : spec.unit === "quantity"
                    ? { label: row.label, value: String(exactInteger(row.value ?? "0")), unit: row.unit ?? "" }
                    : { label: row.label, value: String(exactInteger(row.value ?? "0")) });
        if (roomRateMetric) {
            metricTotalOverride = moneyRatioMinor(sumExact(exactRows.map((row) => row.numerator ?? "0")), sumExact(exactRows.map((row) => row.denominator ?? "0")));
            if (raw.some((row) => exactInteger(row.denominator ?? "0") === BigInt(0) && exactInteger(row.numerator ?? "0") !== BigInt(0)))
                metricPartialOverride = true;
        }
        const quantityUnits = spec.unit === "quantity" ? [...new Set(exactRows.map((row) => "unit" in row ? row.unit ?? "" : ""))] : [];
        const total = metricTotalOverride ?? (spec.unit === "percent"
            ? percentHundredths(sumExact(exactRows.map((row) => "numerator" in row ? row.numerator ?? "0" : "0")), sumExact(exactRows.map((row) => "denominator" in row ? row.denominator ?? "0" : "0")))
            : spec.unit === "quantity" ? quantityUnits.length === 1 ? sumExact(exactRows.map((row) => row.value)) : null : sumExact(exactRows.map((row) => row.value)));
        const quantityRowsTruncated = spec.unit === "quantity" && exactRows.length > 100;
        const hasMetricRows = id === "inventory-shortages" || id === "overdue-project-milestones" || id === "project-dependency-blockers" || id === "project-actual-hours" || id === "project-posted-cost" ? !metricEmptyOverride : id === "exception-resolution-time" || roomRateMetric ? exactRows.length > 0 : raw.length > 0;
        const rows = spec.unit === "quantity"
            ? exactRows.slice(0, 100).map(({ label, value, unit }) => ({ label, value, unit }))
            : roomRateMetric && exactRows.length > 100
                ? [...exactRows.slice(0, 99).map(({ label, value }) => ({ label, value })), {
                        label: "Other",
                        value: moneyRatioMinor(sumExact(exactRows.slice(99).map((row) => row.numerator ?? "0")), sumExact(exactRows.slice(99).map((row) => row.denominator ?? "0"))),
                    }]
                : exactRows.length > 100
                    ? [...exactRows.slice(0, 99).map(({ label, value }) => ({ label, value })), {
                            label: "Other",
                            value: spec.unit === "percent"
                                ? percentHundredths(sumExact(exactRows.slice(99).map((row) => "numerator" in row ? row.numerator ?? "0" : "0")), sumExact(exactRows.slice(99).map((row) => "denominator" in row ? row.denominator ?? "0" : "0")))
                                : sumExact(exactRows.slice(99).map((row) => row.value)),
                        }]
                    : exactRows.map(({ label, value }) => ({ label, value }));
        if (sourceGroup) {
            metricSourceSelection(sourceGroup,raw);
            const selected = rows.find(row => row.label === sourceGroup.label && (sourceGroup.unit === undefined || ('unit' in row && row.unit === sourceGroup.unit)));
            const reported = selected ? formatMetric(selected.value, { unit: spec.unit, currency: spec.unit === 'money' ? f.currency : null, valueScale: spec.unit === 'quantity' ? 3 : spec.unit === 'count' || spec.unit === 'duration' ? 0 : 2 }) + (sourceGroup.unit ? ` ${sourceGroup.unit}` : '') : undefined;
            return snapshot.source(sourceGroup, reported);
        }
        const referenceAt = referenceAtOverride ?? ports.clock.now();
        const watermarks = await snapshot.watermarks();
        const sourceRevision = await ports.digest(JSON.stringify(watermarks));
        const queryDefinitionRevision = `${id}:1`;
        const scopeLevel = spec.holdingOwnerOnly ? "holding" : ["my-tasks", "projects", "overdue-project-milestones", "project-dependency-blockers", "project-actual-hours", "project-posted-cost", "routines", "approved-leave-duration", "authorized-overtime-duration", ...operationsMetrics].includes(id) ? "personal"
            : a.departmentId ? "department" : a.branchId ? "branch" : "company";
        const authorityRevision = await ports.digest(JSON.stringify([a.holdingId, a.companyId, a.assignmentId, a.branchId, a.departmentId, a.iam?.roleRevision, a.iam?.identityVersion]));
        const scope = spec.holdingOwnerOnly ? "Selected Holding billing" : `${a.departmentId ? "Selected department" : a.branchId ? "Selected branch" : "Selected company"}${scopeLevel === "personal" ? " · Current reader only" : ""}`;
        const queryId = await ports.digest(JSON.stringify({ metric: id, queryDefinitionRevision, breakdown, filters: f, scope: [a.holdingId, a.companyId, a.assignmentId, a.branchId, a.departmentId, a.userId] }));
        const revision = await ports.digest(JSON.stringify({ schemaVersion: 2, queryDefinitionRevision, queryId, sourceRevision, rows }));
        const groupedBy = breakdown === "default" ? (id === "invoiced-sales" ? "month" : "adapter default") : breakdown;
        const definition = `${spec.formula} Date basis: ${spec.dateBasis}. Grouping: ${groupedBy}.${id === "headcount" && breakdown !== "default" ? " For people with multiple effective engagements, the latest start date then engagement ID chooses one group." : ""}`;
        return {
            schemaVersion: 2,
            metric: id,
            total,
            rows,
            unit: spec.unit,
            valueScale: spec.unit === "quantity" ? 3 : spec.unit === "count" || spec.unit === "duration" ? 0 : 2,
            currency: spec.unit === "money" ? f.currency : null,
            formula: spec.formula,
            definition,
            scope,
            scopeBoundary: { level: scopeLevel, revision: authorityRevision },
            referenceAt,
            updatedAt: referenceAt,
            sourceRevision,
            queryDefinitionRevision,
            revision,
            status: hasMetricRows ? quantityRowsTruncated || metricPartialOverride ? "partial" : "ready" : "empty",
            sourceReferences: [{ moduleId: spec.moduleId, datasets: spec.sourceTables, queryId, revision: sourceRevision, referencedAt: referenceAt }],
        } satisfies MetricData;
    }
    return { metricCatalogFor, queryMetric, metricUnavailable: (id: MetricId, reason: string, status: 'unavailable' | 'failed' = 'unavailable', currency = 'THB') => unavailable(id, reason, status, currency, ports.clock.now()) };
}
