import {PayrollEnvironment} from "@erpx/feature-people_payroll/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/server-people_payroll/public/payroll-purchases";
import {PageHeader} from "@erpx/ui/components/primitives";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import {PayrollRecoveryConsole} from "@erpx/feature-people_payroll/components/payroll-recovery-console";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {payrollRecoveryPage} from "@erpx/server-people_payroll/public/payroll-recoveries";
import {payrollPostingOptions} from "@erpx/server-people_payroll/public/people-payroll";
export const dynamic='force-dynamic';
export default async function PayrollRecoveries({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{data=await payrollRecoveryPage(actor);}catch(error){return pageError(error,holdingSlug);}
 const purchase=await payrollPurchaseControls(actor);
 const posting=hasPermission(actor,'finance.approve')?await payrollPostingOptions(actor):null;
 return <PayrollEnvironment purchase={purchase}><FeaturePage className="ui-page-content" data-source="live"><PageHeader appearance="standard" title="Payroll recoveries" eyebrow="Employee agreements" description="Specific employee decisions, independent permissibility review, receivables and scheduled deductions."/><PayrollRecoveryConsole base={base} data={data} userId={actor.userId} canPrepare={hasPermission(actor,'payroll.prepare')} canApprove={hasPermission(actor,'payroll.approve')} canRecognize={hasPermission(actor,'finance.approve')} periods={posting?.periods??[]}/></FeaturePage></PayrollEnvironment>;
}
