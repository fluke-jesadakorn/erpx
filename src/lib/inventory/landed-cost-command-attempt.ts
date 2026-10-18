import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type LandedCostCommandAttempt={path:string;body:string;create:boolean;context:string;completion?:WorkCommandCompletion;acknowledged?:(receipt:{id:string;version:number})=>void};
/** A landed cost whose response is uncertain must be resolved under its original
 * context and payload before a newer allocation or decision can become a command. */
export function createLandedCostCommandAttempt() {
  let pending:LandedCostCommandAttempt|null=null;
  const same=(left:LandedCostCommandAttempt,right:LandedCostCommandAttempt)=>left.path===right.path && left.body===right.body && left.context===right.context;
  return {
    select(next:LandedCostCommandAttempt,currentContext:string) {
      if(next.context!==currentContext)throw Error("The workspace changed. Review the original landed cost before retrying.");
      if(pending && !same(pending,next))throw Error("Retry the submitted landed cost change first. Your newer draft is retained.");
      return pending ?? next;
    },
    unresolved(attempt:LandedCostCommandAttempt){pending=attempt;},
    resolved(attempt:LandedCostCommandAttempt){if(!pending || same(pending,attempt))pending=null;},
    retry(){return pending;},
  };
}
