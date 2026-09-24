import { assignmentApplies, requireManageErpxContext } from "@/server/authorization";
import { WorkspaceMenu } from "@/components/workplace/workspace-menu";
import { workspaceChoices } from "@/server/workspace-choices";
import { redirect } from 'next/navigation';
import { safeReturnPath } from '@/lib/access-problem';
import { parseHoldingWorkspace,workspaceBase } from '@/lib/workspace';
import { workRoleOptions } from '@/server/work-role-options';
import { WorkRolePicker } from '@/components/workplace/work-role-picker';
import { pageError } from '@/server/page-error';
export const dynamic='force-dynamic';
export default async function WorkRolePage({searchParams}:PageProps<'/work-role'>){
 const from=safeReturnPath((await searchParams).from);if(!from)redirect('/');
 const parts=new URL(from,'https://erpx.invalid').pathname.split('/').filter(Boolean);
 const parsed=parseHoldingWorkspace(parts[1],parts.slice(2));if(!parsed)redirect('/');
 const {access,options,selection,selected}=await workRoleOptions(parsed.scope).catch(error=>pageError(error,parts[1]));
 if(selected && assignmentApplies(selected,access.scope))redirect(from);
 const [canManageErpx,choices] = await Promise.all([
  requireManageErpxContext().then(()=>true).catch(()=>false),
  workspaceChoices(),
 ]);
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
  <main className="content">
   <section style={{maxWidth:520,margin:"48px auto"}}>
    <h1>Choose your work role</h1>
    <p>Each role uses only its own permissions. Select the company and team scope you need.</p>
    <p>{access.email}</p>
    <WorkRolePicker base={workspaceBase(parsed.scope)} options={options} revision={selection?.revision??0} />
    <WorkspaceMenu label={access.scope.holdingSlug} choices={choices} current={current} canManageErpx={canManageErpx} />
   </section>
  </main>
 );
}
