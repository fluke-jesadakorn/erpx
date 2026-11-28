import {SalesPostingSetup} from '@erpx/sales-ui/components/sales-posting-setup';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {salesPostingSetup} from '@erpx/finance-server/public/sales-posting-setup';
import {salesGoodsPostingSetup} from '@erpx/commerce-server/public/sales-goods-posting-setup';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialGoods]=await Promise.all([salesPostingSetup(actor),salesGoodsPostingSetup(actor)]).catch(error=>pageError(error,holdingSlug));
 return <SalesPostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialGoods={initialGoods}/>;
}
