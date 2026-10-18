import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type ManufacturingCommandAttempt={path:string;body:string;create:boolean;context:string;completion?:WorkCommandCompletion;acknowledged?:(receipt:{id:string;version:number})=>void};
/** A production whose response is uncertain must be resolved under its original
 * context and payload before a newer allocation or decision can become a command. */
export function createManufacturingCommandAttempt() {
  let pending:ManufacturingCommandAttempt|null=null;
  const same=(left:ManufacturingCommandAttempt,right:ManufacturingCommandAttempt)=>left.path===right.path && left.body===right.body && left.context===right.context;
  return {
    select(next:ManufacturingCommandAttempt,currentContext:string) {
      if(next.context!==currentContext)throw Error("The workspace changed. Review the original production before retrying.");
      if(pending && !same(pending,next))throw Error("Retry the submitted production change first. Your newer draft is retained.");
      return pending ?? next;
    },
    unresolved(attempt:ManufacturingCommandAttempt){pending=attempt;},
    resolved(attempt:ManufacturingCommandAttempt){if(!pending || same(pending,attempt))pending=null;},
    retry(){return pending;},
  };
}
