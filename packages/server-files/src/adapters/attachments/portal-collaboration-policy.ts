import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {AttachmentPolicy,AttachmentOperation} from "@erpx/server-files/domain/attachments/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {readablePost} from '@erpx/server-collaboration/public/social-source';
import {getSupportIncident} from '@erpx/server-support/public/support-incident-read';
import {portalAttachmentCustomerAuthorityFacts} from "@erpx/server-customer_authority/public/attachments-portal-facts";
import {portalAttachmentSupportFacts} from "@erpx/server-support/public/attachments-portal-facts";
import {portalAttachmentCollaborationFacts} from "@erpx/server-collaboration/public/attachments-portal-facts";
import {createPortalAttachmentPolicy,type PortalAttachmentUnit} from "@erpx/server-files/application/attachments/portal-collaboration-policy";
function unit(db:SqlDatabase):PortalAttachmentUnit<SqlStatement>{return {readPortalBinding:facts=>portalAttachmentCustomerAuthorityFacts(db).readPortalBinding(facts),readablePost:(actor,id)=>readablePost(db,actor,id),versionGuard:(...args)=>portalAttachmentCustomerAuthorityFacts(db).versionGuard(...args)??portalAttachmentSupportFacts(db).versionGuard(...args)??portalAttachmentCollaborationFacts(db).versionGuard(...args),versionStep:(...args)=>portalAttachmentCollaborationFacts(db).versionStep(...args)};}
const service=createPortalAttachmentPolicy<SqlStatement>({clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),getSupportIncident});
export const portalCollaborationAttachmentPolicies=service.portalCollaborationAttachmentPolicies;
export function authorizePortalCollaborationAttachmentTarget(db: SqlDatabase,actor: WorkContext,policy: AttachmentPolicy,targetId: string,operation: AttachmentOperation){const scoped=createPortalAttachmentPolicy<SqlStatement>({clock:{now:Date.now},error:(status,code,message)=>new ApiError(status,code,message),getSupportIncident:(actor,id)=>getSupportIncident(actor,id,db)});return scoped.authorizePortalCollaborationAttachmentTarget(unit(db),actor,policy,targetId,operation);}
export function portalCollaborationAttachmentTargetVersionGuard(db: SqlDatabase,commandId: string,targetType: string,targetId: string,companyId: string,version: number,index: number){return service.portalCollaborationAttachmentTargetVersionGuard(unit(db),commandId,targetType,targetId,companyId,version,index);}
export function portalCollaborationAttachmentRecordVersionStep(db: SqlDatabase,targetType: string,targetId: string,expectedVersion: number,now = Date.now()){return service.portalCollaborationAttachmentRecordVersionStep(unit(db),targetType,targetId,expectedVersion,now);}
