import {AssetAccounting} from '@erpx/feature-assets/components/asset-accounting';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {assetsFinancialWorkspace} from '@erpx/server-finance/public/assets-financial-workflow';
import {assetsPostingPrerequisites} from '@erpx/server-finance/public/assets-posting-prerequisites';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {pageError} from '@erpx/server-work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),[workspace,prerequisites]=await Promise.all([assetsFinancialWorkspace(actor,id),assetsPostingPrerequisites(actor)]).catch(error=>pageError(error,holdingSlug));
 return <AssetAccounting base={workspaceBase(publicScope(actor))} initial={{workspace,prerequisites}}/>;
}
