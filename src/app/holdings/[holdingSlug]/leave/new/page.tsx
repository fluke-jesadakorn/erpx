import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { LeaveEditor } from "@/components/self-service/leave-editor";
import styles from "@/components/self-service/self-service.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { leaveSummary } from "@/server/leave-self-service";
import Link from "next/link";
export default async function NewLeave({params}:{params:Promise<{holdingSlug:string}>}) {
 const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),summary=await leaveSummary(actor);
 return <FeaturePage className={`content ${styles.workspace}`}><Link href={`${base}/leave`}>← My leave</Link><header className={styles.heading}><div><h1>Request leave</h1><p>Working days and entitlement are calculated from your Company policy.</p></div></header><FeatureTabs/><WidgetGrid pageKey="leave-detail">{summary.setupMessage&&<p role="status">{summary.setupMessage}</p>}<FeatureWidget id="leave-detail-leave-request" title="Leave request"><LeaveEditor key={`${actor.companyId}:${actor.userId}`} base={base} types={summary.balances}/></FeatureWidget></WidgetGrid></FeaturePage>;
}
