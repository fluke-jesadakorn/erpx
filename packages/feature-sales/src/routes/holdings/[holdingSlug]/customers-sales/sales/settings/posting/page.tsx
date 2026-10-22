import {SalesPostingSetup} from '@erpx/feature-sales/components/sales-posting-setup';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {salesPostingSetup} from '@erpx/server-finance/public/sales-posting-setup';
import {salesGoodsPostingSetup} from '@erpx/server-commerce/public/sales-goods-posting-setup';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),[initial,initialGoods]=await Promise.all([salesPostingSetup(actor),salesGoodsPostingSetup(actor)]).catch(error=>pageError(error,holdingSlug));
 return <SalesPostingSetup base={workspaceBase(publicScope(actor))} initial={initial} initialGoods={initialGoods}/>;
}
