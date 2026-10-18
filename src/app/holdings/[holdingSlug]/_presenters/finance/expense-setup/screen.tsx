import {ExpensePostingSetup} from '@/components/records/loaders/modules--expense-posting-setup';
import {PageContent} from '@/components/ui/foundation';
import {hasPermission} from '@/lib/identity_access/permissions';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {publicScope,requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {expensePostingPrerequisites} from '@/server/modules/finance/public/expense-posting-prerequisites';
import {expensePostingSetup} from '@/server/modules/commerce/public/expense-posting-setup';
import {pageError} from '@/server/modules/work_projection/public/page-error';
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
