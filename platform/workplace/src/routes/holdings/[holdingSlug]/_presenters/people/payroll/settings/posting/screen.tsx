import {PayrollPostingPrerequisites} from '@erpx/records-ui/components/loaders/modules--payroll-posting-prerequisites';
import {PageContent} from '@erpx/platform-ui/components/foundation';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {publicScope,requireWorkContext} from '@erpx/identity-access-server/public/authorization';
import {payrollPostingPrerequisites} from '@erpx/finance-server/public/payroll-posting-prerequisites';
import {pageError} from '@erpx/work-projection-server/public/page-error';
import {forbidden} from 'next/navigation';
export const dynamic='force-dynamic';
export default async function PayrollPostingSettings({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'finance.approve'))forbidden();
 let initial:Awaited<ReturnType<typeof payrollPostingPrerequisites>>;
 try{initial=await payrollPostingPrerequisites(actor);}catch(error){return pageError(error,actor.holdingSlug);}
 return <PageContent as="div" className="ui-page-content" data-source="live"><PayrollPostingPrerequisites base={workspaceBase(publicScope(actor))} initial={initial}/></PageContent>;
}
