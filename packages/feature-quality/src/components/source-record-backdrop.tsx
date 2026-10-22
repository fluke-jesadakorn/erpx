import "server-only";
import type {QualitySourceEntry} from "@erpx/server-quality/public/product-entry";
import {sourceRecordPresentation} from "@erpx/feature-records/components/source-record-presentation";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {configuredProductHandoff} from "@erpx/feature-runtime/lib/product-hosts";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import {Card,InlineNotice} from "@erpx/ui/components/foundation";
import {StatusBadge} from "@erpx/ui/components/primitives";

/** The parallel owner record retains its complete original presenter. This
 * background avoids reading an unrelated Supply management collection. */
export async function QualitySourceRecordBackdrop({holdingSlug,kind,id}:{holdingSlug:string;kind:QualitySourceEntry["kind"];id:string}){
 const record=await sourceRecordPresentation(holdingSlug,kind==="hold"?"quality_hold":"stock_count",id).catch(error=>pageError(error,holdingSlug));
 if(!record)return null;
 return <FeaturePage className="ui-page-content" data-source="live" data-module={kind==="hold"?"quality":"inventory"}>
  <header className="ui-section-header"><div><p className="eyebrow">{kind==="hold"?"Source quality hold":"Source disposal count"}</p><h1>{record.title}</h1></div><StatusBadge appearance="standard" status={record.status}/></header>
  <InlineNotice tone="info">Open the record details to review this existing work and its supporting evidence.</InlineNotice>
  <Card appearance="standard" as="section" className="ui-panel"><p>{record.description}</p></Card>
  <a data-product-switch href={configuredProductHandoff("apps")??"/"}>Choose a product in Apps</a>
 </FeaturePage>;
}
