import type {WorkCommandCompletion} from '@erpx/feature-work_projection/lib/work-fetch';

/** Keep the submitted edit revision across exact retries without retaining a
 * detached form for the lifetime of a module's uncertain-command registry. */
export function retainWorkCommandCompletion(completion?:WorkCommandCompletion){
 const form=completion?new WeakRef(completion.form):undefined,version=completion?.formEditVersion??null;
 return ():WorkCommandCompletion|undefined=>{const current=form?.deref();return current?{form:current,formEditVersion:version}:undefined;};
}
