import {collaborationNotificationSnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/collaboration-notification-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import { nativeOrganizationChannels } from "@erpx/server-collaboration/adapters/native-organization-channels";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export async function stageOrganizationChannels(db:SqlDatabase,companyId:string,teamAt:number,roomsAt:number){
  const native = nativeOrganizationChannels(db, companyId, organizationSources.organization_channels_organization_nodes_from);
  return [await native.team(teamAt), await native.room(roomsAt)];
}