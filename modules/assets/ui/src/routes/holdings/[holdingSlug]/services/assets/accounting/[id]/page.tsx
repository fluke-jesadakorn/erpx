import {AssetAccounting} from '@erpx/assets-ui/components/asset-accounting';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {assetsFinancialWorkspace} from '@erpx/finance-server/public/assets-financial-workflow';
import {assetsPostingPrerequisites} from '@erpx/finance-server/public/assets-posting-prerequisites';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {pageError} from '@erpx/work-projection-server/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),[workspace,prerequisites]=await Promise.all([assetsFinancialWorkspace(actor,id),assetsPostingPrerequisites(actor)]).catch(error=>pageError(error,holdingSlug));
 return <AssetAccounting base={workspaceBase(publicScope(actor))} initial={{workspace,prerequisites}}/>;
}
