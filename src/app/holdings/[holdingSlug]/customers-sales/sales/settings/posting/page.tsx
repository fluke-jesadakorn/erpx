import {SalesPostingSetup} from '@/components/sales/sales-posting-setup';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {salesPostingSetup} from '@/server/modules/finance/public/sales-posting-setup';
import {salesGoodsPostingSetup} from '@/server/modules/commerce/public/sales-goods-posting-setup';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialGoods]=await Promise.all([salesPostingSetup(actor),salesGoodsPostingSetup(actor)]).catch(error=>pageError(error,holdingSlug));
 return <SalesPostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialGoods={initialGoods}/>;
}
