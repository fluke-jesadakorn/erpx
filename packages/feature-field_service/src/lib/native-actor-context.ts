import type {ReserveContext} from './native-reserve-command';
/** Pure DTO mapping at Next composition roots; this confers no authority. */
type Actor={userId:string;holdingId:string;companyId:string;branchId:string|null;departmentId:string|null;assignmentId:string;role:string;assignmentScope:ReserveContext['assignmentScopeKind'];modules:readonly string[];authority?:{assignmentVersion:number;revision:number}|null;iam?:{permissions:readonly string[]}|null;scope:{kind:string;branchSlug?:string|null;departmentSlug?:string|null}};
export function nativeActorContext(actor:Actor):ReserveContext|undefined{
 if(!actor.authority||!actor.iam||actor.scope.kind==='department'&&!actor.scope.departmentSlug)return undefined;
 return {userId:actor.userId,holdingId:actor.holdingId,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,branchSlug:actor.scope.kind==='holding'?null:actor.scope.branchSlug??null,departmentSlug:actor.scope.kind==='department'?actor.scope.departmentSlug!:null,assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,revision:actor.authority.revision,role:actor.role,assignmentScopeKind:actor.assignmentScope,modules:[...actor.modules],permissions:[...actor.iam.permissions]};
}
