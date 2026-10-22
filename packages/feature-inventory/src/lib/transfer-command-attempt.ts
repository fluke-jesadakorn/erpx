import type {WorkCommandCompletion} from "@erpx/feature-work_projection/lib/work-fetch";
export type TransferCommandAttempt={path:string;body:string;create:boolean;context:string;completion?:WorkCommandCompletion;acknowledged?:(receipt:{id:string;version:number})=>void};
/** A transfer whose response is uncertain must be resolved under its original
 * context and payload before a newer plan/amendment can become a command. */
export function createTransferCommandAttempt() {
  let pending:TransferCommandAttempt|null=null;
  const same=(left:TransferCommandAttempt,right:TransferCommandAttempt)=>left.path===right.path && left.body===right.body && left.context===right.context;
  return {
    select(next:TransferCommandAttempt,currentContext:string) {
      if(next.context!==currentContext)throw Error("The workspace changed. Review the original transfer before retrying.");
      if(pending && !same(pending,next))throw Error("Retry the submitted transfer change first. Your newer draft is retained.");
      return pending ?? next;
    },
    unresolved(attempt:TransferCommandAttempt){pending=attempt;},
    resolved(attempt:TransferCommandAttempt){if(!pending || same(pending,attempt))pending=null;},
    retry(){return pending;},
  };
}
