import {organizationChannelAssignmentSql} from '../../identity_access/public/organization-channel-assignment-projection';
export function organizationChannelEngagementSql(node:string,user:string){
  return `EXISTS(SELECT 1 FROM hr_people op JOIN hr_engagements oe ON oe.person_id=op.id AND oe.company_id=op.company_id
    WHERE op.company_id=${node}.company_id AND op.user_id=${user} AND op.status='active' AND op.merged_into_id IS NULL
    AND oe.status='active' AND oe.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (oe.end_date IS NULL OR oe.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
    AND (SELECT count(*) FROM hr_engagements other WHERE other.person_id=op.id AND other.company_id=op.company_id AND other.status='active' AND other.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (other.end_date IS NULL OR other.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))=1
    AND (${node}.kind='company' OR (${node}.kind='branch' AND oe.branch_id=${node}.id) OR (${node}.kind='department' AND oe.department_id=${node}.id))
    AND ${organizationChannelAssignmentSql})`;
}
export const organizationChannelPeopleProjection='hr_people p';
