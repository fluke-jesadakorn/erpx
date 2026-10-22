import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { ActiveAssignmentEvidenceRow, PendingAccessReviewEvidenceRow } from '@erpx/server-runtime/public/executive/evidence-contract';
function identityAccessScope(alias: "a" | "i", actor: WorkContext) {
    if (actor.departmentId)
        return {
            sql: `AND (${alias}.scope_kind='holding' OR (${alias}.scope_kind='branch' AND ${alias}.scope_node_id=?) OR (${alias}.scope_kind='department' AND ${alias}.scope_node_id=?))`,
            values: [actor.branchId, actor.departmentId],
        };
    if (actor.branchId)
        return {
            sql: `AND (${alias}.scope_kind='holding' OR (${alias}.scope_kind='branch' AND ${alias}.scope_node_id=?) OR (${alias}.scope_kind='department' AND ${alias}.scope_node_id IN (SELECT id ${organizationSources.executive_main_evidence_sources_organization_nodes_from} WHERE company_id=${alias}.company_id AND branch_id=?)))`,
            values: [actor.branchId, actor.branchId],
        };
    return { sql: "", values: [] as string[] };
}
const activeAssignmentEvidenceSource = `SELECT a.id,a.user_id,a.role,COALESCE(r.name,replace(a.role,'_',' ')) role_name,a.scope_kind,a.scope_node_id,
  CASE WHEN a.scope_kind='holding' THEN 'Company-wide' ELSE COALESCE(n.display_name,a.scope_node_id,'Unavailable scope') END scope_name,
  CASE WHEN a.scope_kind='holding' THEN 'Company-wide' WHEN a.scope_kind='branch' THEN COALESCE(n.display_name,a.scope_node_id,'Unavailable branch') ELSE COALESCE(b.display_name,n.display_name,a.scope_node_id,'Unavailable branch') END branch_name,
  a.source,a.valid_from,a.valid_until,a.version FROM assignments a
  LEFT JOIN LATERAL (SELECT r.name FROM iam_roles r WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' ORDER BY (r.company_id=a.company_id) DESC LIMIT 1) r ON TRUE
  LEFT ${organizationSources.executive_main_evidence_sources_organization_nodes_n_join} ON n.id=a.scope_node_id AND n.company_id=a.company_id AND n.status='active'
  LEFT ${organizationSources.executive_main_evidence_sources_organization_nodes_b_join} ON b.id=CASE WHEN a.scope_kind='branch' THEN a.scope_node_id ELSE n.branch_id END AND b.company_id=a.company_id AND b.status='active'
  WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?)`;
const pendingAccessReviewEvidenceSource = `SELECT i.id,i.campaign_id,i.assignment_id,i.assignment_version,i.user_id,i.role,i.scope_kind,i.scope_node_id,
  CASE WHEN i.scope_kind='holding' THEN 'Company-wide' ELSE COALESCE(n.display_name,i.scope_node_id,'Unavailable scope') END scope_name,
  CASE WHEN i.scope_kind='holding' THEN 'Company-wide' WHEN i.scope_kind='branch' THEN COALESCE(n.display_name,i.scope_node_id,'Unavailable branch') ELSE COALESCE(b.display_name,n.display_name,i.scope_node_id,'Unavailable branch') END branch_name,
  i.source,i.valid_until,i.decision,c.name campaign_name,c.status campaign_status,c.due_at campaign_due_at,c.version campaign_version,c.created_at campaign_created_at
  FROM iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id AND c.company_id=i.company_id
  LEFT ${organizationSources.executive_main_evidence_sources_organization_nodes_n_join} ON n.id=i.scope_node_id AND n.company_id=i.company_id AND n.status='active'
  LEFT ${organizationSources.executive_main_evidence_sources_organization_nodes_b_join} ON b.id=CASE WHEN i.scope_kind='branch' THEN i.scope_node_id ELSE n.branch_id END AND b.company_id=i.company_id AND b.status='active'
  WHERE i.company_id=? AND i.decision='pending' AND c.status='open'`;
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsIdentityAccessUserNames(ids: string[]): Promise<Map<string, string>> {
            const control = (await runtimeEnv()).CONTROL_DB;
            try {
                const rows = (await control.prepare(`SELECT id,name FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`).bind(...ids).all<{
                    id: string;
                    name: string;
                }>()).results;
                return new Map(rows.map((row) => [row.id, row.name]));
            }
            catch (error) {
                // The minimal integration control database does not install Better Auth's
                // user table. Production does; preserve all other database failures.
                if ((error as {
                    code?: string;
                })?.code === "42P01")
                    return new Map<string, string>();
                throw error;
            }
        }, async factsSearchActiveAssignmentEvidence(actor: WorkContext, cursor: {
            userId: string;
            role: string;
            createdAt: undefined;
            recordId: string;
        } | {
            userId: undefined;
            role: undefined;
            createdAt: number;
            recordId: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "active-assignments";
            cursor?: string | undefined;
        }, now: number): Promise<{
            rows: ActiveAssignmentEvidenceRow[];
        }> {
            const scope = identityAccessScope("a", actor);
            const conditions: string[] = [];
            const values: unknown[] = [actor.companyId, now, now, now, ...scope.values];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',a.user_id,a.role,COALESCE(r.name,''),a.scope_kind,COALESCE(n.display_name,''),COALESCE(b.display_name,''),a.source)))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(a.user_id>? OR (a.user_id=? AND (a.role>? OR (a.role=? AND a.id>?))))");
                values.push(cursor.userId, cursor.userId, cursor.role, cursor.role, cursor.recordId);
            }
            const rows = (await db.prepare(`${activeAssignmentEvidenceSource}${scope.sql}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY a.user_id,a.role,a.id LIMIT ?`).bind(...values, input.limit + 1).all<ActiveAssignmentEvidenceRow>()).results;
            return { rows };
        }, async factsReadActiveAssignmentEvidence(actor: WorkContext, now: number, id: string): Promise<{
            row: ActiveAssignmentEvidenceRow | null;
        }> {
            const scope = identityAccessScope("a", actor);
            const row = await db.prepare(`${activeAssignmentEvidenceSource}${scope.sql} AND a.id=? LIMIT 1`).bind(actor.companyId, now, now, now, ...scope.values, id).first<ActiveAssignmentEvidenceRow>();
            return { row };
        }, async factsSearchPendingAccessReviewEvidence(actor: WorkContext, cursor: {
            userId: string;
            role: string;
            createdAt: undefined;
            recordId: string;
        } | {
            userId: undefined;
            role: undefined;
            createdAt: number;
            recordId: string;
        } | null, input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "pending-access-reviews";
            cursor?: string | undefined;
        }): Promise<{
            rows: PendingAccessReviewEvidenceRow[];
        }> {
            const scope = identityAccessScope("i", actor);
            const conditions: string[] = [];
            const values: unknown[] = [actor.companyId, ...scope.values];
            if (input.query) {
                conditions.push("position(lower(?) in lower(concat_ws(' ',c.name,i.user_id,i.role,i.source,i.scope_kind,COALESCE(n.display_name,''),COALESCE(b.display_name,''))))>0");
                values.push(input.query);
            }
            if (cursor) {
                conditions.push("(c.created_at<? OR (c.created_at=? AND i.id>?))");
                values.push(cursor.createdAt, cursor.createdAt, cursor.recordId);
            }
            const rows = (await db.prepare(`${pendingAccessReviewEvidenceSource}${scope.sql}${conditions.length ? ` AND ${conditions.join(" AND ")}` : ""}
    ORDER BY c.created_at DESC,i.id ASC LIMIT ?`).bind(...values, input.limit + 1).all<PendingAccessReviewEvidenceRow>()).results;
            return { rows };
        }, async factsReadPendingAccessReviewEvidence(actor: WorkContext, id: string): Promise<{
            row: PendingAccessReviewEvidenceRow | null;
        }> {
            const scope = identityAccessScope("i", actor);
            const row = await db.prepare(`${pendingAccessReviewEvidenceSource}${scope.sql} AND i.id=? LIMIT 1`).bind(actor.companyId, ...scope.values, id).first<PendingAccessReviewEvidenceRow>();
            return { row };
        } };
}
