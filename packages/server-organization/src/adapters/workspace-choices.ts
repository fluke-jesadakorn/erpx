import { platformOwnerProjectionSources as identity_accessPlatformSources } from "@erpx/server-identity_access/public/platform-owner-projection-sources";
import "server-only";
import {cache} from "react";
import {measureReadPhase} from "@erpx/server-runtime/public/runtime/read-metrics";
import {currentSession} from "@erpx/server-identity_access/public/authorization";
import {iamRoles} from "@erpx/server-identity_access/public/iam-store";
import {activeHoldingBindings,holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {workspaceChoiceBrands} from "@erpx/server-brand/public/workspace-choice-brand";
import {createWorkspaceChoiceApplication} from "@erpx/server-organization/application/workspace-choices";
import type {AssignmentRow,NodeRow} from "@erpx/server-organization/domain/workspace-choices";
function application(){const databases=new Map<string,Promise<SqlDatabase>>(),database=(id:string)=>{let value=databases.get(id);if(!value){value=holdingResources(id).then(x=>x.write);databases.set(id,value);}return value;};return createWorkspaceChoiceApplication({clock:{now:()=>Date.now()},userId:async()=>(await currentSession()).session.user.id,holdings:activeHoldingBindings,
 assignments:async(holdingId,userId,assignmentId,now)=>{const write=await database(holdingId);
  return (
    await write
      .prepare(
        `SELECT a.role,a.company_id,c.display_name AS company_name,a.scope_kind,a.scope_node_id
         FROM (${identity_accessPlatformSources.assignments}) a
         JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
         LEFT JOIN organization_nodes n ON n.id=a.scope_node_id
         WHERE a.user_id=? AND a.revoked_at IS NULL
           ${assignmentId ? "AND a.id=?" : ""}
           AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
           AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
           AND a.valid_from<=? AND a.valid_until>?
           AND (
             a.scope_kind='holding'
             OR (
               n.company_id=a.company_id AND n.status='active'
               AND n.kind=a.scope_kind
               AND (
                 n.branch_id IS NULL
                 OR n.kind='branch'
                 OR EXISTS(
                   SELECT 1 FROM organization_nodes b
                   WHERE b.id=n.branch_id AND b.kind='branch'
                     AND b.company_id=a.company_id AND b.status='active'
                 )
               )
             )
           )
         ORDER BY c.display_name,c.id,a.scope_kind,a.scope_node_id,a.id`,
      )
      .bind(userId, ...(assignmentId ? [assignmentId] : []), now, now)
      .all<AssignmentRow>()
  ).results;

},roleScopes:async(holdingId,companyId,roles)=>new Map([...(await iamRoles(await database(holdingId),companyId,roles))].map(([id,role])=>[id,role.scopes])),nodes:async(holdingId,companyIds)=>{const write=await database(holdingId),placeholders=companyIds.map(()=>'?').join(',');
  return (
    await write
      .prepare(
        `SELECT n.id,n.kind,n.slug,n.display_name,n.company_id,n.branch_id,b.slug AS branch_slug
         FROM organization_nodes n
         LEFT JOIN organization_nodes b
           ON b.id=n.branch_id AND b.kind='branch' AND b.status='active'
         WHERE n.company_id IN (${placeholders})
           AND n.kind IN ('branch','department') AND n.status='active'
         ORDER BY n.display_name,n.id`,
      )
      .bind(...companyIds)
      .all<NodeRow>()
  ).results;
},brands:async(holdingId,companyIds)=>workspaceChoiceBrands(await database(holdingId),companyIds)});}
export const workspaceChoices=cache(()=>measureReadPhase('workspace_choices',()=>application().all()));
export const workspaceChoicesForAssignment=(holdingId:string,assignmentId:string)=>application().forAssignment(holdingId,assignmentId);

/** Product selection lists each accessible Company, Branch and Department explicitly. */
export const productWorkspaceChoices=cache(()=>measureReadPhase('product_workspace_choices',()=>application().all(true)));
