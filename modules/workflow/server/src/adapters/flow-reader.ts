import type { FlowKind, FlowView } from '@erpx/platform-shared-ui/lib/flow';
import type { StaffActor } from '@erpx/operations-server/public/staff-policy';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { createFlowPlanningApplication } from '@erpx/workflow-server/application/flow-routing';
import { nativeFlowPlanningPorts } from '@erpx/workflow-server/adapters/flow-planning-ports';

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
