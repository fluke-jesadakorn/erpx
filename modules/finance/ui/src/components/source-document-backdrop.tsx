import {nativeServicePaymentWorkspace} from '../adapters/server/native-service-payment';
import {nativeServiceInvoiceWorkspace} from "../adapters/server/native-service-invoice";
import {financeProductPresentationOptions} from "@erpx/finance-server/public/product-presentation-request";
import "server-only";
import {requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {financeSourceDocumentBackdrop} from "@erpx/finance-server/public/record-presentation";
import type {FinanceDocumentKind} from "@erpx/finance-server/public/document-purchases";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {configuredProductHandoff} from "@erpx/platform-runtime-ui/lib/product-hosts";
import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import {Card,InlineNotice} from "@erpx/platform-ui/components/foundation";
import {StatusBadge} from "@erpx/platform-ui/components/primitives";

export async function FinanceSourceDocumentBackdrop({holdingSlug,kind,id}:{holdingSlug:string;kind:FinanceDocumentKind;id:string}){
 const actor=await requireWorkContext(holdingSlug);
 const native=kind==='invoice'?await nativeServiceInvoiceWorkspace(actor,holdingSlug,id):undefined;
 const payment=kind==='payment'?await nativeServicePaymentWorkspace(actor,holdingSlug,id):undefined;
 const record=payment?(payment.nativeSourceOnly?{number:String(payment.payment.payment_number),date:String(payment.payment.payment_date),status:String(payment.payment.status),version:Number(payment.payment.version)}:null):native?(native.sourceOnly?{number:String(native.invoice.invoice_number),date:String(native.invoice.invoice_date),status:String(native.invoice.status),version:Number(native.invoice.version)}:null):await financeSourceDocumentBackdrop(actor,kind,id,await financeProductPresentationOptions()).catch(error=>pageError(error,holdingSlug));
 if(!record)return null;
 return <FeaturePage className="ui-page-content" data-source="live" data-module="finance">
  <header className="ui-section-header"><div><p className="eyebrow">Financial source · {kind}</p><h1>{record.number}</h1></div><StatusBadge appearance="standard" status={record.status}/></header>
  <InlineNotice tone="info">This document and its retained history are available through the original product. Open its details to review the source and supporting records.</InlineNotice>
  <Card appearance="standard" as="section" className="ui-panel"><dl className="ui-form-grid"><div><dt>Document date</dt><dd>{record.date}</dd></div><div><dt>Retained version</dt><dd>{record.version}</dd></div></dl></Card>
  <a data-product-switch href={configuredProductHandoff("apps")??"/"}>Choose a product in Apps</a>
 </FeaturePage>;
}
