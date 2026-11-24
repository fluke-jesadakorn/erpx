import 'server-only';

import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

export async function isInitiallyCurrent(database:SqlDatabase,actor:WorkContext){
 const current=currentAssignmentReadScope(actor,'workforce_actor_scope');
 return !!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
}
