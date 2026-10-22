import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
export const executiveScope=(actor:WorkContext)=>JSON.stringify([actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId,actor.iam?.roleRevision,actor.iam?.identityVersion]);
