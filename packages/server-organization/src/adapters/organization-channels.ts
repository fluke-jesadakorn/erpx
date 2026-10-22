import {organizationChannelEngagementSql} from '@erpx/server-people_payroll/public/organization-channel-member-projection';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {stageOrganizationChannels} from "@erpx/server-collaboration/public/organization-channels";
import {createOrganizationChannelApplication} from "@erpx/server-organization/application/organization-channels";
/** The caller supplies a SQL identifier/expression, never end-user SQL. */
export function organizationMemberSql(node:string,user:string){return organizationChannelEngagementSql(node,user);}

export const ensureOrganizationChannels=(db:SqlDatabase,companyId:string)=>createOrganizationChannelApplication({clock:{now:()=>Date.now()},provision:async(id,teamAt,roomsAt)=>db.batch(stageOrganizationChannels(db,id,teamAt,roomsAt))}).ensure(companyId);
