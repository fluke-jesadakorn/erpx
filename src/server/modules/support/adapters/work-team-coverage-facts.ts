import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import type {WorkContext} from "../../identity_access/public/authorization";
export function workTeamCoverageFacts(db:SqlDatabase){return {coveredTeam:(actor:WorkContext)=>db
    .prepare(
      `SELECT 1 FROM service_team_members m
    JOIN service_teams t ON t.id=m.team_id AND t.company_id=m.company_id AND t.active=1
    JOIN service_team_coverage c ON c.team_id=t.id AND c.company_id=t.company_id
    JOIN (${organizationProjectionSources.organization_nodes}) n ON n.id=c.scope_node_id AND n.company_id=c.company_id AND n.status='active'
    LEFT JOIN (${organizationProjectionSources.organization_nodes}) b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.kind='branch'
    WHERE m.company_id=? AND m.user_id=? AND (
      (c.scope_kind='holding' AND n.kind='company' AND n.id=m.company_id) OR
      (c.scope_kind='branch' AND n.kind='branch' AND (CAST(? AS TEXT) IS NULL OR n.id=?)) OR
      (c.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR b.status='active') AND (CAST(? AS TEXT) IS NULL OR n.branch_id=?) AND (CAST(? AS TEXT) IS NULL OR n.id=?))
    ) LIMIT 1`,
    )
    .bind(
      actor.companyId,
      actor.userId,
      actor.branchId,
      actor.branchId,
      actor.branchId,
      actor.branchId,
      actor.departmentId,
      actor.departmentId,
    )
    .first()};}
