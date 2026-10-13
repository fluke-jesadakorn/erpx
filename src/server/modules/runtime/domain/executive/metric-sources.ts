import type {RecordKind} from "@/lib/records/record-drawer";export type MetricSourceGroup={label:string;unit?:string;page:number;recordId?:string};
export type MetricSourceRow={id:string;facts:Record<string,unknown>;owner_id:string|null;owner_kind:RecordKind|null;owner_args:Record<string,string>|null};
export type MetricSourcePage={row:{name:string;reported_total?:string;description:string};rows:MetricSourceRow[];page:number;hasNext:boolean};
import {BusinessRuleError} from '../../../shared';
/** Select the exact group exposed by the aggregate, including its bounded Other remainder. */
export function metricSourceSelection(selection:MetricSourceGroup,raw:{label:string;unit?:string;value?:unknown}[]){
 const matching=raw.filter(row=>row.label===selection.label&&(selection.unit===undefined||selection.unit===row.unit));
 const other=selection.label==='Other'&&!matching.length&&raw.length>100;
 if(!matching.length&&!other)throw new BusinessRuleError(404,'METRIC_GROUP_NOT_FOUND','The selected group is no longer in this report.');
 return {other,labels:other?raw.slice(0,99).map(row=>row.label):[selection.label]};
}
