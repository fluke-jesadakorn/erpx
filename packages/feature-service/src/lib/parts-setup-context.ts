import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {ReserveContext} from '@erpx/feature-field_service/lib/native-reserve-command';
export function partsSetupContext(actor:WorkContext):ReserveContext|undefined{
 if(!actor.authority||!actor.iam)return undefined;
 return {userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:actor.scope.kind==='holding'?null:actor.scope.branchSlug??null,departmentSlug:actor.scope.kind==='department'?actor.scope.departmentSlug:null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope,modules:[...actor.modules],permissions:[...actor.iam.permissions]};
}
