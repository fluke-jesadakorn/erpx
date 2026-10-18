import {AssetAccounting} from '@/components/assets/asset-accounting';
import {requireWorkContext,publicScope} from '@/server/modules/identity_access/public/authorization';
import {assetsFinancialWorkspace} from '@/server/modules/finance/public/assets-financial-workflow';
import {assetsPostingPrerequisites} from '@/server/modules/finance/public/assets-posting-prerequisites';
import {workspaceBase} from '@/lib/work_projection/workspace';
import {pageError} from '@/server/modules/work_projection/public/page-error';
export const dynamic='force-dynamic';
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),[workspace,prerequisites]=await Promise.all([assetsFinancialWorkspace(actor,id),assetsPostingPrerequisites(actor)]).catch(error=>pageError(error,holdingSlug));
 return <AssetAccounting base={workspaceBase(publicScope(actor))} initial={{workspace,prerequisites}}/>;
}
