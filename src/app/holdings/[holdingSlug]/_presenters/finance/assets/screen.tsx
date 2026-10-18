import {financePurchaseControls} from "@/server/modules/finance/public/purchases";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import {FeaturePage} from "@/components/shared/features/feature-page";
import {FinanceAssetConsole} from "@/components/finance/finance-asset-console";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {hasPermission} from "@/lib/identity_access/permissions";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {assetPage,assetOptions} from "@/server/modules/finance/public/finance-assets";
import {pageError} from "@/server/modules/work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetsPage({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [assets,options]=await Promise.all([assetPage(actor),assetOptions(actor)]);data={assets,...options};}catch(error){return pageError(error,holdingSlug);}const purchases=await financePurchaseControls(actor,["finance.accounting","finance.asset-recognition","finance.asset-disposal"]);return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace purchases={purchases} base={base} active="accounting" title="Fixed assets" description="Acquisition, reviewed book policies, monthly depreciation and retained disposal history"><FinanceLegacyHeading><Link href={canonicalUiHref(`${base}/finance/engines`)}>← Finance engines</Link><h1>Fixed assets, depreciation and disposal</h1></FinanceLegacyHeading><FinanceAssetConsole base={base} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FinanceRecordWorkspace></FeaturePage>;}
