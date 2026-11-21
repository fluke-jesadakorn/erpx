import 'server-only';
// Projection bridge only: the Rust report owns Field SQL, source eligibility,
// cursor/filter policy and access audit. Retained shared consumer: the
// Runtime/Apps executive canvas metric projection. Removal condition: retire
// that generic metric engine or give it native report sources directly.
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Breakdown,Filters,MetricId} from '@erpx/server-runtime/public/rules/canvas-domain';
import type {MetricSnapshot} from '@erpx/server-runtime/public/executive/metric-projection-contract';
import {metricSourceSelection} from '@erpx/server-runtime/public/executive/metric-sources';
import {specs} from '@erpx/server-runtime/public/executive/metric-projection-contract';
import {readNativeFieldReport} from './native-executive';
import {ApiError} from '@erpx/server-runtime/public/http';
import {BusinessRuleError} from '@erpx/server-shared/index';
export async function readExecutiveMetricProjection(_sourceDb:SqlDatabase,actor:WorkContext,id:MetricId,filters:Filters,breakdown:Breakdown):Promise<MetricSnapshot>{
 void _sourceDb;
 if(id!=='open-service-orders'&&id!=='service-billing-releases'&&id!=='field-visit-completion')throw new ApiError(404,'METRIC_NOT_FOUND','This Field metric is unavailable.');
 const query={metric:id,breakdown,fromAt:Date.parse(filters.from),toAt:Date.parse(filters.to)};
 const data=await readNativeFieldReport(actor,query);
 return {raw:data.raw,referenceAtOverride:null,watermarks:async()=>[],source:async(selection,reported)=>{
  const current=await readNativeFieldReport(actor,query);let selected:ReturnType<typeof metricSourceSelection>;try{selected=metricSourceSelection(selection,current.raw);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}const labels=new Set(selected.labels);
  // Projection only: every row and group has already been authorized in Rust,
  // with PostgreSQL source ordering preserved. This does not grant record access.
  const rows=current.sources.filter(row=>selected.other?!labels.has(row.label):labels.has(row.label)).filter(row=>!selection.recordId||row.id===selection.recordId);
  const start=selection.recordId?0:(selection.page-1)*30,selectedRows=rows.slice(start,start+(selection.recordId?1:31)).map(({label,...row})=>{void label;return row;});
  return {row:{name:selection.label,reported_total:reported,description:`${specs[id].dateBasis} · ${filters.from} to ${filters.to} · ${filters.currency}`},rows:selectedRows,page:selection.page,hasNext:selectedRows.length>30};
 }};
}
