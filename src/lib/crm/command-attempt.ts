import type {WorkCommandCompletion} from "@/lib/work_projection/work-fetch";
export type CRMCommandAttempt={path:string;body:string;context:string;completion?:WorkCommandCompletion};
export function createCRMCommandAttempt(){let pending:CRMCommandAttempt|null=null;const same=(left:CRMCommandAttempt,right:CRMCommandAttempt)=>left.path===right.path&&left.body===right.body&&left.context===right.context;return {
 select(next:CRMCommandAttempt,currentContext:string){if(next.context!==currentContext)throw Error('The workspace changed. Return to the original role before retrying.');if(pending&&!same(pending,next))throw Error('Retry the submitted customer change first. Your newer draft is retained.');return pending??next;},
 unresolved(attempt:CRMCommandAttempt){pending=attempt;},resolved(attempt:CRMCommandAttempt){if(!pending||same(pending,attempt))pending=null;},retry(){return pending;},
};}
