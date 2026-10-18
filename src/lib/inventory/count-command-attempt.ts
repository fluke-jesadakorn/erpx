import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type CountCommandAttempt={path:string;body:string;create:boolean;context:string;completion?:WorkCommandCompletion;acknowledged?:(receipt:{id:string;version:number})=>void};
/** A count whose response is uncertain must be resolved under its original
 * context and payload before a newer creation/recount can become a command. */
export function createCountCommandAttempt() {
  let pending:CountCommandAttempt|null=null;
  const same=(left:CountCommandAttempt,right:CountCommandAttempt)=>left.path===right.path && left.body===right.body && left.context===right.context;
  return {
    select(next:CountCommandAttempt,currentContext:string) {
      if(next.context!==currentContext)throw Error("The workspace changed. Review the original count before retrying.");
      if(pending && !same(pending,next))throw Error("Retry the submitted count change first. Your newer draft is retained.");
      return pending ?? next;
    },
    unresolved(attempt:CountCommandAttempt){pending=attempt;},
    resolved(attempt:CountCommandAttempt){if(!pending || same(pending,attempt))pending=null;},
    retry(){return pending;},
  };
}
