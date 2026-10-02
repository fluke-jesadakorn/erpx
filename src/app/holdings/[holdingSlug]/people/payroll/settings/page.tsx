import Link from "next/link";
import { PayrollRunEditor } from "@/components/modules/payroll-controls";
import { PayrollAccountMappingSettings } from "@/components/modules/payroll-accounting-panel";
import { PayrollScheduleSettings } from "@/components/modules/payroll-schedule-settings";
import { PayrollAlert, PayrollHeading, PayrollNote, PayrollPageIntro } from "@/components/modules/payroll-workspace";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { payrollOptions, payrollPostingOptions } from "@/server/people-payroll";
import { getPayrollSchedule } from "@/server/payroll-readiness";
import { runtimeEnv } from "@/server/runtime";
import { isThaiHotelPayrollMockEnabled } from "@/server/simulation-only-adapters";
import { forbidden } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function PayrollSettings({params}: {params: Promise<{holdingSlug:string}>}) {
  const {holdingSlug}=await params;
  const actor=await requireWorkContext(holdingSlug);
  const canPrepare=hasPermission(actor,"payroll.prepare"), canMap=hasPermission(actor,"finance.approve");
  if ((!canPrepare && !canMap) || actor.branchId || actor.departmentId) forbidden();
  const base=workspaceBase(publicScope(actor));
  let groups: Awaited<ReturnType<typeof payrollOptions>>=[];
  let postingOptions: Awaited<ReturnType<typeof payrollPostingOptions>>|null=null;
  let schedules: Awaited<ReturnType<typeof getPayrollSchedule>>[]=[];
  let mappingUnavailable=false,scheduleUnavailable=false;
  try { if(canPrepare) groups=await payrollOptions(actor); }
  catch(error) { return pageError(error, actor.holdingSlug); }
  if(canMap) {
    try { postingOptions=await payrollPostingOptions(actor); if(!canPrepare) groups=postingOptions.groups; }
    catch { mappingUnavailable=true; }
  }
  if(canPrepare) {
    const results=await Promise.all(groups.map(group=>getPayrollSchedule(actor,String(group.id)).catch(()=>null)));
    scheduleUnavailable=results.some(value=>value===null);
    schedules=results.filter((value):value is NonNullable<typeof value>=>value!==null);
  }
  return <main className="content-stack" data-source="live"><PayrollPageIntro base={base} title="Settings" description="Manage scheduled runs and accounting. Pay group policies are configured in People."/>
    {canPrepare && <p><Link className="button secondary" href={`${base}/people?panel=settings&section=pay-groups`}>Manage pay groups in People</Link></p>}
    {scheduleUnavailable&&<PayrollAlert message="Payroll schedules could not load. Ask an administrator to check the Payroll database migration."/>}
    {canPrepare && <PayrollScheduleSettings base={base} userId={actor.userId} schedules={schedules} names={Object.fromEntries(groups.map(group=>[String(group.id),String(group.name)]))}/>}
    {mappingUnavailable&&<PayrollAlert message="Payroll account mappings could not load. Ask an administrator to check the Finance database migration."/>}
    {canMap && postingOptions && <PayrollAccountMappingSettings base={base} groups={groups} options={postingOptions} userId={actor.userId}/>}
    {canPrepare && ((await runtimeEnv()).ERPX_ENVIRONMENT !== "production" ? <section className="card"><PayrollHeading title="Manual run"/><PayrollNote text="Use this to test previews while scheduled runs are being prepared."/><PayrollRunEditor base={base} userId={actor.userId} groups={groups} simulationMode={isThaiHotelPayrollMockEnabled(actor.holdingSlug)}/></section> : <p className="notice">Production creation awaits statutory and bank acceptance.</p>)}
  </main>;
}
