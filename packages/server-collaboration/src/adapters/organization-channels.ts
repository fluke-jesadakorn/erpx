import {collaborationNotificationSnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/collaboration-notification-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function stageOrganizationChannels(db:SqlDatabase,companyId:string,teamAt:number,roomsAt:number){return [
    db.prepare(`INSERT INTO messaging_teams(id,company_id,name,owner_id,organization_managed,created_at) SELECT 'org-team-' || company_id,company_id,'Organization','organization',true,? ${organizationSources.organization_channels_organization_nodes_from} WHERE company_id=? AND kind='company' AND status='active' ON CONFLICT DO NOTHING`).bind(teamAt,companyId),
    db.prepare(`INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,team_id,organization_node_id,created_at) SELECT 'org-channel-' || n.id,n.company_id,'channel',n.display_name,'organization','org-team-' || n.company_id,n.id,? ${organizationSources.organization_channels_organization_nodes_n_from} WHERE n.company_id=? AND n.kind IN ('company','branch','department') AND n.status='active' AND EXISTS(SELECT 1 FROM messaging_teams t WHERE t.id='org-team-' || n.company_id) AND NOT EXISTS(SELECT 1 FROM messaging_rooms r WHERE r.company_id=n.company_id AND r.organization_node_id=n.id) ON CONFLICT DO NOTHING`).bind(roomsAt,companyId),
];}
