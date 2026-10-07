import {collaborationNotificationSnapshotSources as organizationSnapshotSources} from '../../organization/public/collaboration-notification-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function stageOrganizationNodeChannels(db:SqlDatabase,companyId:string,displayName:string,nodeId:string|null,now:number){    return [
      db.prepare("INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) VALUES(?,?,'Organization','organization',true,?) ON CONFLICT DO NOTHING").bind(`org-team-${companyId}`,companyId,now),
      db.prepare(`INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) SELECT ?,?,'channel',?,'organization',?,?,? WHERE EXISTS(SELECT 1 ${organizationSources.organization_node_channels_organization_nodes_from} WHERE id=? AND company_id=? AND status='active') ON CONFLICT DO NOTHING`).bind(`org-channel-${nodeId}`,companyId,displayName,`org-team-${companyId}`,nodeId,now,nodeId,companyId),
      db.prepare("UPDATE messaging_rooms SET name=? WHERE company_id=? AND organization_node_id=?").bind(displayName,companyId,nodeId),
    ];}
