import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/records/loaders/finance--finance-record-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import {FeaturePage} from "@/components/features/feature-page";
import { FinanceAssetConsole } from "@/components/records/loaders/modules--finance-asset-console";
import {workspaceBase} from "@/lib/workspace";
import {hasPermission} from "@/lib/permissions";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {assetDetail,assetOptions} from "@/server/modules/finance/public/finance-assets";
import {pageError} from "@/server/modules/work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function FinanceAssetPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));let data;try{const [detail,options]=await Promise.all([assetDetail(actor,id),assetOptions(actor)]);data={detail,...options};}catch(error){return pageError(error,holdingSlug);}return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace base={base} active="accounting" title={`${data.detail.asset.asset_number} · ${data.detail.asset.name}`} description="Fixed asset record and retained accounting history"><FinanceLegacyHeading><Link href={`${base}/finance/assets`}>← Finance fixed assets</Link><h1>Fixed asset record and accounting history</h1></FinanceLegacyHeading><FinanceAssetConsole base={base} assets={[]} {...data} userId={actor.userId} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/></FinanceRecordWorkspace></FeaturePage>;}
