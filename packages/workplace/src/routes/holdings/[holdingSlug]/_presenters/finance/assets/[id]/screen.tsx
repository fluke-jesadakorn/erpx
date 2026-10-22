import {financePurchaseControls} from "@erpx/server-finance/public/purchases";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@erpx/feature-records/components/loaders/finance--finance-record-workspace";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import { FinanceAssetConsole } from "@erpx/feature-records/components/loaders/modules--finance-asset-console";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {publicScope,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {assetDetail,assetOptions} from "@erpx/server-finance/public/finance-assets";
import {pageError} from "@erpx/server-work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [detail,options]=await Promise.all([assetDetail(actor,id),assetOptions(actor)]);data={detail,...options};}catch(error){return pageError(error,holdingSlug);}const purchases=await financePurchaseControls(actor,["finance.accounting","finance.asset-recognition","finance.asset-disposal"]);return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace purchases={purchases} base={base} active="accounting" title={`${data.detail.asset.asset_number} · ${data.detail.asset.name}`} description="Fixed asset record and retained accounting history"><FinanceLegacyHeading><Link href={canonicalUiHref(`${base}/finance/assets`)}>← Finance fixed assets</Link><h1>Fixed asset record and accounting history</h1></FinanceLegacyHeading><FinanceAssetConsole base={base} assets={[]} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FinanceRecordWorkspace></FeaturePage>;}
