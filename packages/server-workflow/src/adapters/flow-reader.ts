import type { FlowKind, FlowView } from '@erpx/feature-shared/lib/flow';
import type { StaffActor } from '@erpx/server-operations/public/staff-policy';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { createFlowPlanningApplication } from '@erpx/server-workflow/application/flow-routing';
import { nativeFlowPlanningPorts } from '@erpx/server-workflow/adapters/flow-planning-ports';

export interface FlowReadOwner {
  requireHistory(database:SqlDatabase,actor:StaffActor,id:string):Promise<void>;
  decorate(view:FlowView,database:SqlDatabase,actor:StaffActor):Promise<FlowView>|FlowView;
}
/** Composition supplies the fixed document kind and its existing owner policy.
 * Each invocation uses its caller's DB/actor; no command capability is loaded. */
export function createNativeFlowReader(kind:FlowKind,owner:FlowReadOwner){
  return async(database:SqlDatabase,actor:StaffActor,id:string):Promise<FlowView>=>{
    await owner.requireHistory(database,actor,id);
    let view:FlowView;
    try{view=await createFlowPlanningApplication(nativeFlowPlanningPorts(database)).resolveFlow(actor,kind,id);}
    catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
    return owner.decorate(view,database,actor);
  };
}
