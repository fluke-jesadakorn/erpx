import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type QualityCommandAttempt={path:string;body:string;create:boolean;context:string;completion?:WorkCommandCompletion;acknowledged?:(receipt:{id:string;version:number})=>void};
/** A quality whose response is uncertain must be resolved under its original
 * context and payload before a newer plan/amendment can become a command. */
export function createQualityCommandAttempt() {
  let pending:QualityCommandAttempt|null=null;
  const same=(left:QualityCommandAttempt,right:QualityCommandAttempt)=>left.path===right.path && left.body===right.body && left.context===right.context;
  return {
    select(next:QualityCommandAttempt,currentContext:string) {
      if(next.context!==currentContext)throw Error("The workspace changed. Review the original quality before retrying.");
      if(pending && !same(pending,next))throw Error("Retry the submitted quality change first. Your newer draft is retained.");
      return pending ?? next;
    },
    unresolved(attempt:QualityCommandAttempt){pending=attempt;},
    resolved(attempt:QualityCommandAttempt){if(!pending || same(pending,attempt))pending=null;},
    retry(){return pending;},
  };
}
