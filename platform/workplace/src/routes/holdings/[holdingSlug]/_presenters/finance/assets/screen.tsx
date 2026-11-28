import {financePurchaseControls} from "@erpx/finance-server/public/purchases";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@erpx/finance-ui/components/finance-record-workspace";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import {FinanceAssetConsole} from "@erpx/finance-ui/components/finance-asset-console";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {publicScope,requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {assetPage,assetOptions} from "@erpx/finance-server/public/finance-assets";
import {pageError} from "@erpx/work-projection-server/public/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetsPage({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [assets,options]=await Promise.all([assetPage(actor),assetOptions(actor)]);data={assets,...options};}catch(error){return pageError(error,holdingSlug);}const purchases=await financePurchaseControls(actor,["finance.accounting","finance.asset-recognition","finance.asset-disposal"]);return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace purchases={purchases} base={base} active="accounting" title="Fixed assets" description="Acquisition, reviewed book policies, monthly depreciation and retained disposal history"><FinanceLegacyHeading><Link href={canonicalUiHref(`${base}/finance/engines`)}>← Finance engines</Link><h1>Fixed assets, depreciation and disposal</h1></FinanceLegacyHeading><FinanceAssetConsole base={base} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FinanceRecordWorkspace></FeaturePage>;}
