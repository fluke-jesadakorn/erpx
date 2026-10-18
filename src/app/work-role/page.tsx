import {workRoleDisplayChoices} from "@/lib/identity_access/work-role-display";

import { PageContent } from "@/components/ui/patterns";
import { assignmentApplies, canEnterManageErpx } from "@/server/modules/identity_access/public/authorization";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { workspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { redirect } from 'next/navigation';
import { safeReturnPath } from "@/lib/shared/access-problem";
import { parseHoldingWorkspace,workspaceBase } from "@/lib/work_projection/workspace";
import { workRoleOptions } from "@/server/modules/work_projection/public/work-role-options";
import { WorkRolePicker } from '@/components/workplace/work-role-picker';
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { accountQuickAccessItems } from "@/server/modules/identity_access/public/account-quick-access";
import { readQuickAccessPreferences } from "@/server/modules/work_projection/public/quick-access-preferences";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import rail from "@/components/work_projection/quick-access/area-rail.module.css";
export const dynamic='force-dynamic';
export default async function WorkRolePage({searchParams}:PageProps<'/work-role'>){
 const from=safeReturnPath((await searchParams).from);if(!from)redirect('/');
 const parts=new URL(from,'https://erpx.invalid').pathname.split('/').filter(Boolean);
 const parsed=parseHoldingWorkspace(parts[1],parts.slice(2));if(!parsed)redirect('/');
 const {access,options,selection,selected}=await workRoleOptions(parsed.scope).catch(error=>pageError(error,parts[1]));
 if(selected && assignmentApplies(selected,access.scope))redirect(from);
 const [canManageErpx,choices,preferences] = await Promise.all([
  canEnterManageErpx().catch(()=>false),
  workspaceChoices(),
  readQuickAccessPreferences(access.userId,'account-work-role',workspaceBase(parsed.scope)),
 ]);
 const items=accountQuickAccessItems('work-role',{hasWorkspace:choices.length>0,hasRoles:options.length>0});
 const current =
  access.scope.kind === "holding"
   ? {
      kind: "holding" as const,
      holdingSlug: access.scope.holdingSlug,
      companyId: access.scope.companyId,
     }
   : access.scope.kind === "branch"
    ? {
       kind: "branch" as const,
       holdingSlug: access.scope.holdingSlug,
       companyId: access.scope.companyId,
       branchId: access.scope.branchId,
      }
    : {
       kind: "department" as const,
       holdingSlug: access.scope.holdingSlug,
       companyId: access.scope.companyId,
       branchId: access.scope.branchId,
       departmentId: access.scope.departmentId,
      };
 return (
  <>
  <a href="#work-role-content" className="skip-link">Skip to work role</a>
  <PageContent as="main" style={{maxWidth:1360}}>
   <header style={{maxWidth:700,marginBottom:24}}>
    <h1>Choose your work role</h1>
    <p>Each role uses only its own permissions. Select the company and team scope you need.</p>
    <p>{access.email}</p>
   </header>
   <div className={rail.layout}>
   <aside className={rail.rail} aria-label="Quick access">
    <QuickAccessShelf appearance="standard" items={items} initialPreferences={preferences} defaultIds={items.map(item=>item.id)} endpoint={`/api/v1/account/quick-access/work-role?from=${encodeURIComponent(from)}`} requestMode="plain" embedded placement="rail" />
   </aside>
   <div id="work-role-content" tabIndex={-1} className={rail.content}>
   <section style={{display:"grid",gap:16,maxWidth:520}}>
    <div id="role-picker"><WorkRolePicker base={workspaceBase(parsed.scope)} options={workRoleDisplayChoices(options)} revision={selection?.revision??0} from={from} companyId={access.scope.companyId} activeAssignmentId={selected?.id} /></div>
    <div id="available-workspaces"><WorkspaceMenu appearance="standard" label={access.scope.holdingSlug} choices={choices} current={current} canManageErpx={canManageErpx} /></div>
   </section>
   </div>
   </div>
  </PageContent>
  </>
 );
}
