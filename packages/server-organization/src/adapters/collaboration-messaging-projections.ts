import {organizationMemberSql} from '@erpx/server-organization/public/organization-channels';
export function messagingManagedTeamSql(alias:string,user:string){return `EXISTS(SELECT 1 FROM organization_nodes n WHERE n.company_id=${alias}.company_id AND n.kind='company' AND n.status='active' AND ${organizationMemberSql('n',user)})`;}
export function messagingRoomOrganizationSql(alias:string,user:string){return `EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=${alias}.organization_node_id AND n.company_id=${alias}.company_id AND n.status='active' AND ${organizationMemberSql('n',user)})`;}
