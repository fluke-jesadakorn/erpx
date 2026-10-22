import {financeProductPresentationOptions} from "@erpx/server-finance/public/product-presentation-request";
import 'server-only';
import {financeDocumentPresentationAccess} from '@erpx/server-finance/public/record-presentation';
import {configuredProductHandoff} from '@erpx/feature-runtime/lib/product-hosts';
import {Suspense,type ReactNode} from 'react';
import {requireWorkContext,publicScope} from '@erpx/server-identity_access/public/authorization';
import {workspaceBase} from '@erpx/feature-work_projection/lib/workspace';
import {recordCanonicalHref,recordRoutes,type RecordTarget} from '@erpx/feature-records/lib/record-drawer';
import {RecordDrawer} from '@erpx/feature-records/components/record-drawer';
import {sourceRecordPresentation} from '@erpx/feature-records/components/source-record-presentation';
export async function RecordDetailBoundary({holdingSlug,target,children}:{holdingSlug:string;target:RecordTarget;children:ReactNode}) {
 const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const included=target.kind==="quality_hold"||target.kind==="stock_count"||target.kind==="staff_task"?await sourceRecordPresentation(holdingSlug,target.kind,target.id):null;
 const sourceOnly=!!included||(target.kind==="invoice"||target.kind==="journal"||target.kind==="payment")&&(await financeDocumentPresentationAccess(actor,target.kind,target.id,await financeProductPresentationOptions())).sourceOnly;
 return <RecordDrawer closeHref={sourceOnly?configuredProductHandoff("apps")??undefined:undefined} key={`${base}:${actor.assignmentId}:${target.kind}:${target.id}`} title={recordRoutes[target.kind].label} fullHref={recordCanonicalHref(base,target)}><Suspense fallback={<p role="status">Loading record details…</p>}>{children}</Suspense></RecordDrawer>;
}
