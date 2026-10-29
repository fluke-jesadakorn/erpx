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
import {ApiError} from '@erpx/server-runtime/public/http';
export async function RecordDetailBoundary({holdingSlug,target,children}:{holdingSlug:string;target:RecordTarget;children:ReactNode}) {
 const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const included=target.kind==="quality_hold"||target.kind==="stock_count"||target.kind==="staff_task"?await sourceRecordPresentation(holdingSlug,target.kind,target.id):null;
 let sourceOnly=!!included;
 if(target.kind==="invoice"||target.kind==="journal"||target.kind==="payment"){
  try{sourceOnly=(await financeDocumentPresentationAccess(actor,target.kind,target.id,await financeProductPresentationOptions())).sourceOnly;}
  catch(error){
   if(!(error instanceof ApiError)||![403,404].includes(error.status))throw error;
   return <RecordDrawer title="Record unavailable" fullHref={recordCanonicalHref(base,target)}><p role="alert">This record is unavailable in your current workspace.</p><p>Close details to return to your list.</p></RecordDrawer>;
  }
 }
 return <RecordDrawer closeHref={sourceOnly?configuredProductHandoff("apps")??undefined:undefined} key={`${base}:${actor.assignmentId}:${target.kind}:${target.id}`} title={recordRoutes[target.kind].label} fullHref={recordCanonicalHref(base,target)}><Suspense fallback={<p role="status">Loading record details…</p>}>{children}</Suspense></RecordDrawer>;
}
