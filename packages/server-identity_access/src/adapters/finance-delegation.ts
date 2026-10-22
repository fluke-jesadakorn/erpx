import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { loadIamAuthority } from '@erpx/server-identity_access/public/iam-store';
type Row = Record<string, string | number | null>;
export async function financeDelegatedAssignment(db: SqlDatabase, actor: WorkContext, now: number) { return db.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?").bind(actor.assignmentId, actor.userId, actor.companyId, actor.role, now, now).first<{
    version: number;
}>(); }
export async function financeDueAssignment(db: SqlDatabase, row: Row, now: number) { return db.prepare(`SELECT a.version FROM assignments a ${organizationSources.finance_delegation_organization_nodes_n_join} ON n.id=a.company_id AND n.status='active' WHERE a.id=? AND a.user_id=? AND a.company_id=? AND a.role=? AND a.scope_kind='holding' AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?`).bind(row.assignment_id, row.delegated_by, row.company_id, row.role, now, now).first<{
    version: number;
}>(); }
export async function financeDueAuthority(db: SqlDatabase, row: Row) { return loadIamAuthority(db, String(row.company_id), String(row.delegated_by), String(row.role)); }
