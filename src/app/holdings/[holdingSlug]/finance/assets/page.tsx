import Link from "next/link";
import {FeaturePage} from "@/components/features/feature-page";
import {FinanceAssetConsole} from "@/components/modules/finance-asset-console";
import {workspaceBase} from "@/lib/workspace";
import {hasPermission} from "@/lib/permissions";
import {publicScope,requireWorkContext} from "@/server/authorization";
import {assetPage,assetOptions} from "@/server/finance-assets";
import {pageError} from "@/server/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetsPage({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [assets,options]=await Promise.all([assetPage(actor),assetOptions(actor)]);data={assets,...options};}catch(error){return pageError(error,holdingSlug);}return <FeaturePage className="ui-page-content" data-source="live"><Link href={`${base}/finance/engines`}>← Finance engines</Link><h1>Fixed assets, depreciation and disposal</h1><FinanceAssetConsole base={base} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FeaturePage>;}
