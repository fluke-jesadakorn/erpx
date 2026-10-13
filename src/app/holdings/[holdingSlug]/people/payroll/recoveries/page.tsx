import {PageHeader} from "@/components/ui/primitives";
import {FeaturePage} from "@/components/shared/features/feature-page";
import {PayrollRecoveryConsole} from "@/components/people_payroll/payroll-recovery-console";
import {hasPermission} from "@/lib/identity_access/permissions";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {payrollRecoveryPage} from "@/server/modules/people_payroll/public/payroll-recoveries";
import {payrollPostingOptions} from "@/server/modules/people_payroll/public/people-payroll";
export const dynamic='force-dynamic';
export default async function PayrollRecoveries({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{data=await payrollRecoveryPage(actor);}catch(error){return pageError(error,holdingSlug);}
 const posting=hasPermission(actor,'finance.approve')?await payrollPostingOptions(actor):null;
 return <FeaturePage className="ui-page-content" data-source="live"><PageHeader appearance="standard" title="Payroll recoveries" eyebrow="Employee agreements" description="Specific employee decisions, independent permissibility review, receivables and scheduled deductions."/><PayrollRecoveryConsole base={base} data={data} userId={actor.userId} canPrepare={hasPermission(actor,'payroll.prepare')} canApprove={hasPermission(actor,'payroll.approve')} canRecognize={hasPermission(actor,'finance.approve')} periods={posting?.periods??[]}/></FeaturePage>;
}
