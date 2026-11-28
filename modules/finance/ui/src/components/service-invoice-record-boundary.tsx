import 'server-only';
import {Suspense,type ReactNode} from 'react';
import {requireWorkContext,publicScope} from '@erpx/identity-access-server/public/authorization';
import {configuredProductHandoff} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {workspaceBase} from '@erpx/work-projection-ui/lib/workspace';
import {recordCanonicalHref,recordRoutes,type RecordTarget} from '@erpx/records-ui/lib/record-drawer';
import {RecordDrawer} from '@erpx/records-ui/components/record-drawer';
import {RecordDetailBoundary} from '@erpx/records-ui/components/record-detail-boundary';
import {NativeFinanceReadError} from '../adapters/server/native-journal-detail';
import {nativeServiceInvoiceWorkspace} from '../adapters/server/native-service-invoice';

export async function ServiceInvoiceRecordBoundary({holdingSlug,target,children}:{holdingSlug:string;target:RecordTarget;children:ReactNode}) {
 const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 let native;
 try {native=await nativeServiceInvoiceWorkspace(actor,holdingSlug,target.id);}
 catch(error){
  if(!(error instanceof NativeFinanceReadError)||![403,404].includes(error.status??0))throw error;
  return <RecordDrawer title="Record unavailable" fullHref={recordCanonicalHref(base,target)}><p role="alert">This record is unavailable in your current workspace.</p><p>Close details to return to your list.</p></RecordDrawer>;
 }
 if(!native)return <RecordDetailBoundary holdingSlug={holdingSlug} target={target}>{children}</RecordDetailBoundary>;
 const sourceOnly=native.sourceOnly;
 return <RecordDrawer closeHref={sourceOnly?configuredProductHandoff('apps')??undefined:undefined} key={`${base}:${actor.assignmentId}:${target.kind}:${target.id}`} title={recordRoutes[target.kind].label} fullHref={recordCanonicalHref(base,target)}><Suspense fallback={<p role="status">Loading record details…</p>}>{children}</Suspense></RecordDrawer>;
}
