import { z } from "zod";
import { definitionSchema, filtersSchema, type Filters, type ShareView } from "@/server/modules/runtime/public/rules/canvas-domain";
import { BusinessRuleError } from '../../../shared';
export type ReportRow = {
    id: string;
    company_id: string;
    owner_id: string;
    definition_json: string;
    filters_json: string;
    version: number;
    updated_at: number;
};
export type ShareRow = {
    id: string;
    report_id: string;
    company_id: string;
    recipient_id: string;
    recipient_name: string;
    role: "viewer" | "editor";
    mode: "recipient" | "distribution";
    widget_ids_json: string;
    definition_json: string;
    filters_json: string;
    publisher_json: string;
    expires_at: number | null;
    revoked_at: number | null;
    version: number;
};
export const missing = () => new BusinessRuleError(404, "REPORT_NOT_FOUND", "This report is unavailable or has not been shared with you.");
export const shareView = (s: ShareRow): ShareView => ({ id: s.id, recipientId: s.recipient_id, name: s.recipient_name, role: s.role, mode: s.mode, widgetIds: JSON.parse(s.widget_ids_json), expiresAt: s.expires_at, version: s.version });
export const saveSchema = z.object({ expectedVersion: z.number().int().positive(), definition: definitionSchema, filters: filtersSchema }).strict();
export function previousPeriod(filters: Filters): Filters {
    const from = Date.parse(`${filters.from}T00:00:00Z`), to = Date.parse(`${filters.to}T00:00:00Z`), days = (to - from) / 86400000 + 1;
    const previousTo = from - 86400000, previousFrom = previousTo - (days - 1) * 86400000;
    return filtersSchema.parse({ from: new Date(previousFrom).toISOString().slice(0, 10), to: new Date(previousTo).toISOString().slice(0, 10), currency: filters.currency });
}
export type PreviewAssignmentRow = {
    id: string;
    role: string;
    modules_json: string;
    company_id: string;
    branch_id: string | null;
    version: number;
    valid_until: number;
    scope_kind: "holding" | "branch" | "department";
    scope_node_id: string | null;
    scope_name: string | null;
    scope_slug: string | null;
    scope_branch_id: string | null;
    scope_branch_name: string | null;
    scope_branch_slug: string | null;
};
export type PreviewAssignment = {
    row: PreviewAssignmentRow;
    roleName: string;
    label: string;
    scope: string;
    current: boolean;
    permissions: string[];
};
