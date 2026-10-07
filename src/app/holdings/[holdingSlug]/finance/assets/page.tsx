import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import {FeaturePage} from "@/components/features/feature-page";
import {FinanceAssetConsole} from "@/components/modules/finance-asset-console";
import {workspaceBase} from "@/lib/workspace";
import {hasPermission} from "@/lib/permissions";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {assetPage,assetOptions} from "@/server/modules/finance/public/finance-assets";
import {pageError} from "@/server/modules/work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetsPage({params}:{params:Promise<{holdingSlug:string}>}){const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [assets,options]=await Promise.all([assetPage(actor),assetOptions(actor)]);data={assets,...options};}catch(error){return pageError(error,holdingSlug);}return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace base={base} active="accounting" title="Fixed assets" description="Acquisition, reviewed book policies, monthly depreciation and retained disposal history"><FinanceLegacyHeading><Link href={`${base}/finance/engines`}>← Finance engines</Link><h1>Fixed assets, depreciation and disposal</h1></FinanceLegacyHeading><FinanceAssetConsole base={base} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FinanceRecordWorkspace></FeaturePage>;}
