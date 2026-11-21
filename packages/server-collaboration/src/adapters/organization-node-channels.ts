import { nativeOrganizationNodeChannels } from "@erpx/server-collaboration/adapters/native-organization-node-channels";
import {collaborationNotificationSnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/collaboration-notification-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export async function stageOrganizationNodeChannels(db:SqlDatabase,companyId:string,displayName:string,nodeId:string|null,now:number){
  const native = nativeOrganizationNodeChannels(db, companyId, organizationSources.organization_node_channels_organization_nodes_from);
  return [await native.team(now), await native.room({nodeId,displayName,now}), await native.rename({displayName,nodeId})];
}