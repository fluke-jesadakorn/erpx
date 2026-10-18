import {financeProductPresentationOptions} from "@/server/modules/finance/public/product-presentation-request";
import "server-only";
import {requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {financeSourceDocumentBackdrop} from "@/server/modules/finance/public/record-presentation";
import type {FinanceDocumentKind} from "@/server/modules/finance/public/document-purchases";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {configuredProductHandoff} from "@/lib/runtime/product-hosts";
import {FeaturePage} from "@/components/shared/features/feature-page";
import {Card,InlineNotice} from "@/components/ui/foundation";
import {StatusBadge} from "@/components/ui/primitives";

export async function FinanceSourceDocumentBackdrop({holdingSlug,kind,id}:{holdingSlug:string;kind:FinanceDocumentKind;id:string}){
 const actor=await requireWorkContext(holdingSlug);
 const record=await financeSourceDocumentBackdrop(actor,kind,id,await financeProductPresentationOptions()).catch(error=>pageError(error,holdingSlug));
 if(!record)return null;
 return <FeaturePage className="ui-page-content" data-source="live" data-module="finance">
  <header className="ui-section-header"><div><p className="eyebrow">Financial source · {kind}</p><h1>{record.number}</h1></div><StatusBadge appearance="standard" status={record.status}/></header>
  <InlineNotice tone="info">This document and its retained history are available through the original product. Open its details to review the source and supporting records.</InlineNotice>
  <Card appearance="standard" as="section" className="ui-panel"><dl className="ui-form-grid"><div><dt>Document date</dt><dd>{record.date}</dd></div><div><dt>Retained version</dt><dd>{record.version}</dd></div></dl></Card>
  <a data-product-switch href={configuredProductHandoff("apps")??"/"}>Choose a product in Apps</a>
 </FeaturePage>;
}
