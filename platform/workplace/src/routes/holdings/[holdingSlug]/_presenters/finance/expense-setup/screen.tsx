import {ExpensePostingSetup} from '@erpx/records-ui/components/loaders/modules--expense-posting-setup';
import {PageContent} from '@erpx/platform-ui/components/foundation';
import {hasPermission} from '@erpx/identity-access-ui/lib/permissions';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {publicScope,requireWorkContext} from '@erpx/identity-access-server/public/authorization';
import {expensePostingPrerequisites} from '@erpx/finance-server/public/expense-posting-prerequisites';
import {expensePostingSetup} from '@erpx/commerce-server/public/expense-posting-setup';
import {pageError} from '@erpx/work-projection-server/public/page-error';
import {forbidden} from 'next/navigation';
export const dynamic='force-dynamic';
export default async function ExpensePostingSettings({params}:{params:Promise<{holdingSlug:string}>}){
 const actor=await requireWorkContext((await params).holdingSlug);
 if(actor.branchId||actor.departmentId||!hasPermission(actor,'finance.read')||!['finance.write','finance.approve'].some(permission=>hasPermission(actor,permission)))forbidden();
 let initial:{prerequisites:Awaited<ReturnType<typeof expensePostingPrerequisites>>;mapping:Awaited<ReturnType<typeof expensePostingSetup>>};
 // Each original owner provides its own source context; no combined atomic snapshot is claimed.
 try{initial={prerequisites:await expensePostingPrerequisites(actor),mapping:await expensePostingSetup(actor)};}catch(error){return pageError(error,actor.holdingSlug);}
 return <PageContent as="div" className="ui-page-content" data-source="live"><ExpensePostingSetup base={workspaceBase(publicScope(actor))} initial={initial}/></PageContent>;
}
