import "server-only";
import {closedProjection} from './closed-projection-source';
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  legal_events: closedProjection(['id','item_id','action','actor_id','note','version','created_at','snapshot'], ',', 'legal_events'),
  legal_items: closedProjection(['id','company_id','branch_id','department_id','kind','title','description','classification','country','jurisdiction','language','currency','timezone','priority','due_date','owner_id','created_by','state','version','created_at','updated_at','escalation_owner','sla_started_at','sla_due_at','intake_form_id','intake_answers'], ',', 'legal_items'),
  legal_native_references: closedProjection(['item_id','target_module','target_id','ordinal','created_by','created_at'], ',', 'legal_native_references'),
} as const;
