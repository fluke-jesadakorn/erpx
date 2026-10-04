import Link from "next/link";
import {FeaturePage} from "@/components/features/feature-page";
import {FinanceAssetConsole} from "@/components/modules/finance-asset-console";
import {workspaceBase} from "@/lib/workspace";
import {hasPermission} from "@/lib/permissions";
import {publicScope,requireWorkContext} from "@/server/authorization";
import {assetDetail,assetOptions} from "@/server/finance-assets";
import {pageError} from "@/server/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [detail,options]=await Promise.all([assetDetail(actor,id),assetOptions(actor)]);data={detail,...options};}catch(error){return pageError(error,holdingSlug);}return <FeaturePage className="ui-page-content" data-source="live"><Link href={`${base}/finance/assets`}>← Finance fixed assets</Link><h1>Fixed asset record and accounting history</h1><FinanceAssetConsole base={base} assets={[]} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FeaturePage>;}
