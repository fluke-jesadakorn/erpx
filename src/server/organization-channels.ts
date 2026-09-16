import type { SqlDatabase } from "./runtime/types";

/** The caller supplies a SQL identifier/expression, never end-user SQL. */
export function organizationMemberSql(node:string,user:string) {
  return `EXISTS(SELECT 1 FROM hr_people op JOIN hr_engagements oe ON oe.person_id=op.id AND oe.company_id=op.company_id
    WHERE op.company_id=${node}.company_id AND op.user_id=${user} AND op.status='active' AND op.merged_into_id IS NULL
    AND oe.status='active' AND oe.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (oe.end_date IS NULL OR oe.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
    AND (SELECT count(*) FROM hr_engagements other WHERE other.person_id=op.id AND other.company_id=op.company_id AND other.status='active' AND other.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (other.end_date IS NULL OR other.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1
    AND (${node}.kind='company' OR (${node}.kind='branch' AND oe.branch_id=${node}.id) OR (${node}.kind='department' AND oe.department_id=${node}.id))
    AND EXISTS(SELECT 1 FROM assignments oa WHERE oa.company_id=op.company_id AND oa.user_id=op.user_id AND oa.revoked_at IS NULL AND oa.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND oa.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
}

// Idempotent channel provisioning. Authorization never relies on a copied member list.
export async function ensureOrganizationChannels(db:SqlDatabase,companyId:string) {
  await db.batch([
    db.prepare("INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) SELECT 'org-team-' || company_id,company_id,'Organization','organization',true,? FROM organization_nodes WHERE company_id=? AND kind='company' AND status='active' ON CONFLICT DO NOTHING").bind(Date.now(),companyId),
    db.prepare("INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) SELECT 'org-channel-' || n.id,n.company_id,'channel',n.display_name,'organization','org-team-' || n.company_id,n.id,? FROM organization_nodes n WHERE n.company_id=? AND n.kind IN ('company','branch','department') AND n.status='active' AND EXISTS(SELECT 1 FROM messaging_teams t WHERE t.id='org-team-' || n.company_id) AND NOT EXISTS(SELECT 1 FROM messaging_rooms r WHERE r.company_id=n.company_id AND r.organization_node_id=n.id) ON CONFLICT DO NOTHING").bind(Date.now(),companyId),
  ]);
}
