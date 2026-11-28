import {organizationChannelEngagementSql} from '@erpx/people-payroll-server/public/organization-channel-member-projection';
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {stageOrganizationChannels} from "@erpx/collaboration-server/public/organization-channels";
import {createOrganizationChannelApplication} from "@erpx/organization-server/application/organization-channels";
/** The caller supplies a SQL identifier/expression, never end-user SQL. */
export function organizationMemberSql(node:string,user:string){return organizationChannelEngagementSql(node,user);}

export const ensureOrganizationChannels=(db:SqlDatabase,companyId:string)=>createOrganizationChannelApplication({clock:{now:()=>Date.now()},provision:async(id,teamAt,roomsAt)=>db.batch(await stageOrganizationChannels(db,id,teamAt,roomsAt))}).ensure(companyId);
