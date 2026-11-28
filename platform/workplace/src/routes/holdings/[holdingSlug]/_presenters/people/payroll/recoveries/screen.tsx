import {PayrollEnvironment} from "@erpx/people-payroll-ui/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/people-payroll-server/public/payroll-purchases";
import {PageHeader} from "@erpx/platform-ui/components/primitives";
import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import {PayrollRecoveryConsole} from "@erpx/people-payroll-ui/components/payroll-recovery-console";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {payrollRecoveryPage} from "@erpx/people-payroll-server/public/payroll-recoveries";
import {payrollPostingOptions} from "@erpx/people-payroll-server/public/people-payroll";
export const dynamic='force-dynamic';
export default async function PayrollRecoveries({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{data=await payrollRecoveryPage(actor);}catch(error){return pageError(error,holdingSlug);}
 const purchase=await payrollPurchaseControls(actor);
 const posting=hasPermission(actor,'finance.approve')?await payrollPostingOptions(actor):null;
 return <PayrollEnvironment purchase={purchase}><FeaturePage className="ui-page-content" data-source="live"><PageHeader appearance="standard" title="Payroll recoveries" eyebrow="Employee agreements" description="Specific employee decisions, independent permissibility review, receivables and scheduled deductions."/><PayrollRecoveryConsole base={base} data={data} userId={actor.userId} canPrepare={hasPermission(actor,'payroll.prepare')} canApprove={hasPermission(actor,'payroll.approve')} canRecognize={hasPermission(actor,'finance.approve')} periods={posting?.periods??[]}/></FeaturePage></PayrollEnvironment>;
}
