import {organizationMemberSql} from '@erpx/organization-server/public/organization-channels';
import {messagingNodeExists} from './organization-read-tail-sources-r49';
export function messagingManagedTeamSql(alias:string,user:string){return messagingNodeExists(`n.company_id=${alias}.company_id AND n.kind='company' AND n.status='active' AND ${organizationMemberSql('n',user)}`);}
export function messagingRoomOrganizationSql(alias:string,user:string){return messagingNodeExists(`n.id=${alias}.organization_node_id AND n.company_id=${alias}.company_id AND n.status='active' AND ${organizationMemberSql('n',user)}`);}
