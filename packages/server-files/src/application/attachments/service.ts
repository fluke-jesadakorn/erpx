import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {Clock,IdentifierSource} from "@erpx/server-shared/application/ports";
import type {AttachmentEnvironment} from "@erpx/server-files/domain/attachments/feature";
import {ATTACHMENT_DRAFT_MS,ATTACHMENT_RESERVATION_MS} from "@erpx/server-files/domain/attachments/feature";
import type {AttachmentComponentKind,AttachmentLinkState,AttachmentSourceState,AttachmentSetDto,AttachmentLinkDto,AttachmentManifestItem,AttachmentBindingContract,AttachmentPolicy,AttachmentTargetScope,AttachmentOperation,PreparedAttachmentBinding} from "@erpx/server-files/domain/attachments/types";
import type {SetRow,LinkRow} from "@erpx/server-files/domain/attachments/service-models";
import {createAttachmentPolicy} from "@erpx/server-files/application/attachments/policy";
import {createFinanceAttachmentPolicy} from "@erpx/server-files/application/attachments/finance-operations-policy";
import type {detectedAttachmentMediaType as detectMediaSignature} from "@erpx/server-files/domain/attachments/media-signature";
import type {AttachmentByteStore} from "@erpx/server-files/domain/file-object";
export type AttachmentResult<Row>={results:Row[]};
export type AttachmentServiceUnit<Effect>={legacyWriterGuard(kind:"set"|"upload"|"link"|"attachment",id:string):Promise<void>;legacyWriterEffect(kind:"set"|"upload"|"link"|"attachment",id:string):Effect;legacyCleanupAtomic<Result>(companyId:string,keys:string[],action:(unit:AttachmentServiceUnit<Effect>)=>Promise<Result>):Promise<Result>;legacyCleanupSourceAllowed(sourceId:string):Promise<boolean>;legacyCleanupObjectAllowed(companyId:string,key:string):Promise<boolean>;legacyScanGuard(facts:{sourceId:string;jobId:string;lease:string}):Effect;legacyScanMark(facts:{sourceId:string}):Effect;ownerVersionGuard(commandId:string,targetType:string,targetId:string,actor:WorkContext,version:number,index:number):Effect;ownerVersionStep(targetType:string,targetId:string,expectedVersion:number,now:number,context?:{actor:WorkContext;commandId:string}):Effect;readAttachmentSetAlias(facts:{id:string;companyId:string;userId:string}):Promise<SetRow|null>;stepAttachmentSetAlias(facts:{id:string;companyId:string;userId:string;setId:string;now:number}):Effect;readAttachmentSets1(facts:{id:string;companyId:string;}):Promise<SetRow | null>;readActiveWorkContexts2(facts:{sessionId:string;companyId:string;}):Promise<{ assignment_id: string | null; assignment_version: number | null; revision: number; expires_at: number; user_id: string; } | null>;readAssignments3(facts:{assignmentId:string;assignmentVersion:number;userId:string;role:string;companyId:string;branchId:string | null;departmentId:string | null;}):Promise<{ id: string; } | null>;readLeaveRequests4(facts:{targetId:string;companyId:string;}):Promise<{ id: string; } | null>;readAttachmentLinks5(facts:{id:string;companyId:string;}):Promise<AttachmentResult<LinkRow & { intent_original_name: string | null; intent_media_type: string | null; }>>;readAttachmentEvents6(facts:{id:string;company_id:string;}):Promise<Record<string,unknown>|null>;readAttachmentSets7(facts:{actor:WorkContext;input:{targetType:string;targetId:string;purposeKey?:string;componentKind?:AttachmentComponentKind;expectedTargetVersion?:number};readOnly:boolean;leaveReview:boolean;}):Promise<AttachmentResult<SetRow>>;lock8(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentSets9(facts:{id:string;companyId:string;}):Promise<SetRow | null>;readAttachmentEvents10(facts:{id:string;companyId:string;}):Promise<Record<string,unknown>|null>;readAttachmentSets11(facts:{companyId:string;targetType:string;targetId:string;componentKind:"record" | "action" | "line" | "event" | "message" | "publication";componentRef:string | null;purposeKey:string;expectedTargetVersion:number;userId:string;arg9:boolean;}):Promise<SetRow | null>;stepAttachmentSets12(facts:{id:string;companyId:string;branchId:string | null;departmentId:string | null;targetType:string;targetId:string;componentKind:"record" | "action" | "line" | "event" | "message" | "publication";componentRef:string | null;purposeKey:string;expectedTargetVersion:number;version:number;classification:import("@erpx/server-files/domain/attachments/types").AttachmentClassification;arg13:string;key:string | null;versionValue15:number | null;retainUntil:null;destructionMode:"review" | null;requiredCount:number;maxFiles:number;userId:string;now:number;}):Effect;stepAttachmentEvents13(facts:{arg1:string;companyId:string;id:string;userId:string;arg5:"set.created";arg6:string;now:number;}):Effect;readAttachmentLinks14(facts:{supersedesLinkId:string;companyId:string;target_type:string;target_id:string;purpose_key:string;}):Promise<Record<string, unknown> | null>;lock15(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readUploadIntents16(facts:{token:string;companyId:string;userId:string;}):Promise<{ attachment_set_id: string; attachment_link_id: string; original_name: string; media_type: string; declared_bytes: number; expires_at: number; caption: string | null; supersedes_link_id: string | null; } | null>;readAttachmentLinks17(facts:{setId:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentStorageLedger18(facts:{companyId:string;}):Promise<Record<string, unknown> | null>;readUploadIntents19(facts:{companyId:string;now:number;}):Promise<Record<string, unknown> | null>;readBetaPolicy20(facts:{companyId:string;}):Promise<{ storage_bytes: number; } | null>;stepAttachmentLinks21(facts:{linkId:string;setId:string;caption:string | null;arg4:string | null;userId:string;now:number;draftExpires:number;}):Effect;stepUploadIntents22(facts:{token:string;target_type:string;target_id:string;purpose_key:string;filename:string;mediaType:string;bytes:number;objectKey:string;arg9:"pending";userId:string;reserveExpires:number;now:number;expected_target_version:number;setId:string;linkId:string;companyId:string;}):Effect;stepAttachmentLinks23(facts:{token:string;linkId:string;}):Effect;stepAttachmentEvents24(facts:{arg1:string;companyId:string;linkId:string;setId:string;userId:string;arg6:"file.reserved";arg7:string;now:number;}):Effect;readUploadIntents25(facts:{uploadIntentId:string;userId:string;companyId:string;arg4:number;}):Promise<Record<string, unknown> | null>;readAttachments26(facts:{attachmentId:string;companyId:string;}):Promise<{ checksum_sha256: string; source_state: AttachmentSourceState; } | null>;lock27(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readUploadIntents28(facts:{uploadIntentId:string;userId:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachments29(facts:{attachmentId:string;companyId:string;}):Promise<{ checksum_sha256: string; source_state: AttachmentSourceState; } | null>;readAttachmentLinks30(facts:{attachment_link_id:unknown;attachment_set_id:unknown;companyId:string;}):Promise<{ attachment_id: string | null; } | null>;readAttachmentLinks31(facts:{attachment_link_id:unknown;attachment_set_id:unknown;companyId:string;}):Promise<{ state: string; set_id: string; } | null>;readAttachmentSets32(facts:{attachment_set_id:unknown;companyId:string;}):Promise<{ state: string; } | null>;stepAttachments33(facts:{attachmentId:string;uploadIntentId:string;owner_type:unknown;owner_id:unknown;purpose:unknown;objectKey:string;revision:unknown;original_name:unknown;media_type:unknown;byteLength:number;checksum:string;userId:string;now:number;companyId:string;detected:"image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";}):Effect;stepUploadIntents34(facts:{byteLength:number;checksum:string;detected:"image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";objectKey:string;now:number;uploadIntentId:string;companyId:string;}):Effect;stepAttachmentLinks35(facts:{attachmentId:string;attachment_link_id:unknown;companyId:string;ownerType:unknown;}):Effect;stepAttachmentScanAttempts36(facts:{arg1:string;attachmentId:string;now:number;}):Effect;stepAttachmentStorageLedger37(facts:{arg1:string;companyId:string;attachmentId:string;objectKey:string;byteLength:number;now:number;}):Effect;stepAttachmentEvents38(facts:{arg1:string;companyId:string;attachmentId:string;attachment_link_id:unknown;attachment_set_id:unknown;userId:string;arg7:"file.quarantined";arg8:string;now:number;}):Effect;readAttachmentLinks39(facts:{linkId:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentScanAttempts40(facts:{attachment_id:unknown;}):Promise<Record<string, unknown> | null>;writeAttachmentAccessEvents41(facts:{arg1:string;companyId:string;attachment_id:unknown;linkId:string;userId:string;arg6:number;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentLinks42(facts:{linkId:string;companyId:string;}):Promise<Record<string, unknown> | null>;writeAttachmentAccessEvents43(facts:{arg1:string;companyId:string;attachment_id:unknown;linkId:string;userId:string;variant:"original" | "preview";arg7:number;}):Promise<AttachmentResult<Record<string, unknown>>>;writeAttachmentAccessEvents44(facts:{arg1:string;companyId:string;attachment_id:unknown;linkId:string;userId:string;variant:"original" | "preview";arg7:string;arg8:number;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentLinks45(facts:{linkId:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentLinks46(facts:{linkId:string;companyId:string;}):Promise<Record<string, unknown> | null>;stepAttachmentLinks47(facts:{now:number;userId:string;text:string;linkId:string;companyId:string;}):Effect;stepAttachmentEvents48(facts:{arg1:string;companyId:string;attachment_id:unknown;linkId:string;set_id:unknown;userId:string;arg7:"link.withdrawn";arg8:string;now:number;}):Effect;lock49(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentEvents50(facts:{eventId:string;companyId:string;userId:string;}):Promise<{ set_id: string | null; details_json: string; } | null>;readAttachmentSets51(facts:{setId:string;companyId:string;}):Promise<{ state: string; } | null>;readAttachmentLinks52(facts:{setId:string;companyId:string;}):Promise<AttachmentResult<{ id: string; source_id: string | null; original_name: string | null; checksum_sha256: string | null; media_type: string | null; bytes: number | null; source_state: AttachmentSourceState | null; caption: string | null; }>>;stepAttachmentEvents53(facts:{eventId:string;companyId:string;setId:string;userId:string;arg5:"sanitization.attested";details:string;now:number;}):Effect;stepAttachmentSets54(facts:{now:number;setId:string;companyId:string;}):Effect;readAttachmentLinks55(facts:{sourceLinkId:string;companyId:string;}):Promise<Record<string, unknown> | null>;lock56(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentSets57(facts:{setId:string;companyId:string;}):Promise<SetRow | null>;readAttachmentLinks58(facts:{id:string;companyId:string;}):Promise<{ set_id: string; attachment_id: string; caption: string | null; } | null>;readAttachmentLinks59(facts:{setId:string;attachment_id:unknown;companyId:string;}):Promise<{ id: string; } | null>;readAttachmentLinks60(facts:{setId:string;companyId:string;}):Promise<Record<string, unknown> | null>;stepAttachmentLinks61(facts:{id:string;setId:string;attachment_id:unknown;caption:string | null;userId:string;now:number;arg7:number;}):Effect;stepAttachmentEvents62(facts:{arg1:string;companyId:string;attachment_id:unknown;id:string;setId:string;userId:string;arg7:"link.reused";arg8:string;now:number;}):Effect;readAttachmentSets63(facts:{companyId:string;setIds:(string)[];}):Promise<Record<string, unknown> | null>;readAttachmentLinks64(facts:{id:string;companyId:string;}):Promise<AttachmentResult<{ link_id: string; attachment_id: string | null; checksum_sha256: string | null; media_type: string | null; bytes: number | null; source_state: AttachmentSourceState | null; }>>;readAttachmentEvents65(facts:{id:string;companyId:string;}):Promise<Record<string, unknown> | null>;stepAttachmentLinks66(facts:{now:number;bindingType:string;bindingId:string;link_id:string;companyId:string;}):Effect;stepAttachmentSets67(facts:{now:number;commandId:string;id:string;companyId:string;}):Effect;stepAttachmentEvents68(facts:{arg1:string;companyId:string;id:string;userId:string;arg5:"set.bound";arg6:string;now:number;}):Effect;stepOrganizationBrandProfiles71(facts:{kind:"holding" | "branch";targetId:string;arg3:string | null;arg4:"holding" | "branch";userId:string;now:number;publicationId:string;purposeKey:"inverse" | "primary";}):Effect;stepStorefrontAppBrands72(facts:{userId:string;now:number;publicationId:string;targetId:string;}):Effect;stepOrganizationBrandProfiles73(facts:{arg1:"holding" | "branch";targetId:string;publicationId:string;purposeKey:"inverse" | "primary";}):Effect;stepStorefrontAppBrands74(facts:{targetId:string;publicationId:string;}):Effect;readAttachmentLinks75(facts:{sourceLinkId:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentPublications76(facts:{id:string;companyId:string;}):Promise<Record<string, unknown> | null>;lock77(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentPublications78(facts:{id:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentPublications79(facts:{companyId:string;targetType:string;targetId:string;}):Promise<Record<string, unknown> | null>;stepAttachmentPublications80(facts:{id:string;companyId:string;sourceLinkId:string;targetType:string;targetId:string;key:string;preview_media_type:unknown;checksum:string;byteLength:number;alt:string;ordinal:number;revision:number;userId:string;now:number;}):Effect;stepAttachmentPublicationOrders81(facts:{companyId:string;targetType:string;targetId:string;userId:string;now:number;}):Effect;stepAttachmentStorageLedger82(facts:{arg1:string;companyId:string;attachment_id:unknown;key:string;byteLength:number;now:number;}):Effect;stepAttachmentEvents83(facts:{arg1:string;companyId:string;attachment_id:unknown;sourceLinkId:string;userId:string;arg6:"publication.created";arg7:string;now:number;}):Effect;readAttachmentPublications84(facts:{id:string;}):Promise<Record<string, unknown> | null>;readAttachmentPublications85(facts:{companyId:string;targetType:string;targetId:string;canManage:boolean;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentPublicationOrders86(facts:{companyId:string;targetType:string;targetId:string;}):Promise<{ version: number; } | null>;readPublicationRevocation(facts:{eventId:string;companyId:string;userId:string;publicationId:string;reason:string;}):Promise<{id:string}|null>;readAttachmentPublications87(facts:{id:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentPublications88(facts:{id:string;companyId:string;}):Promise<Record<string, unknown> | null>;stepAttachmentPublications89(facts:{userId:string;now:number;text:string;id:string;companyId:string;}):Effect;stepAttachmentEvents90(facts:{arg1:string;companyId:string;source_link_id:unknown;userId:string;arg5:"publication.revoked";arg6:string;now:number;}):Effect;lock92(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentEvents93(facts:{eventId:string;companyId:string;userId:string;}):Promise<{ details_json: string; } | null>;readAttachmentPublicationOrders94(facts:{companyId:string;targetType:string;targetId:string;}):Promise<{ version: number; } | null>;readAttachmentPublications95(facts:{companyId:string;targetType:string;targetId:string;}):Promise<AttachmentResult<{ id: string; }>>;stepAttachmentPublications96(facts:{ordinal:number;id:string;companyId:string;}):Effect;stepAttachmentPublicationOrders97(facts:{companyId:string;targetType:string;targetId:string;userId:string;now:number;expectedVersion:number;}):Effect;stepAttachmentEvents98(facts:{eventId:string;companyId:string;userId:string;arg4:"publication.reordered";arg5:string;now:number;}):Effect;readAttachmentPublications99(facts:{id:string;companyId:string;}):Promise<Record<string, unknown> | null>;readAttachmentPublications100(facts:{id:string;companyId:string;}):Promise<{ state: string; } | null>;stepAttachmentPublications102(facts:{userId:string;now:number;id:string;companyId:string;}):Effect;stepAttachmentEvents103(facts:{arg1:string;companyId:string;source_link_id:unknown;userId:string;arg5:"publication.approved";arg6:string;now:number;}):Effect;readAttachmentPublications105(facts:{id:string;}):Promise<{ derivative_object_key: string; derivative_media_type: string; derivative_checksum_sha256: string; alt_text: string; revision: number; } | null>;readAttachmentScanAttempts106(facts:{now:number;arg2:number;lease:string;arg4:string | null;arg5:string | null;}):Promise<Record<string, unknown> | null>;readAttachments107(facts:{attachment_id:unknown;}):Promise<Record<string, unknown> | null>;writeAttachments108(facts:{id:unknown;}):Promise<AttachmentResult<Record<string, unknown>>>;stepAttachmentScanAttempts109(facts:{engine:string;signature:string | null;definitionUpdatedAt:number;arg4:number;id:unknown;lease:string;}):Effect;stepAttachments110(facts:{detected:"image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";arg2:number;definitionUpdatedAt:number;id:unknown;}):Effect;stepAttachmentEvents111(facts:{arg1:string;company_id:unknown;id:unknown;arg4:"system:jobs";arg5:"scan.malware_rejected";arg6:string;arg7:number;}):Effect;stepAttachmentScanAttempts112(facts:{engine:string;definitionUpdatedAt:number;completed:number;id:unknown;lease:string;}):Effect;stepAttachments113(facts:{detected:"image/png" | "image/jpeg" | "image/webp" | "image/heic" | "image/heif" | "application/pdf" | "text/csv" | "application/vnd.openxmlformats-officedocument.wordprocessingml.document";completed:number;definitionUpdatedAt:number;previewKey:string | null;arg5:"image/jpeg" | "image/webp" | null;previewChecksum:string | null;arg7:number | null;id:unknown;}):Effect;stepUploadIntents114(facts:{completed:number;upload_id:unknown;}):Effect;stepAttachmentLinks115(facts:{arg1:number;upload_id:unknown;}):Effect;stepAttachmentEvents116(facts:{arg1:string;company_id:unknown;id:unknown;arg4:"system:jobs";arg5:"scan.available";arg6:string;completed:number;}):Effect;stepAttachmentStorageLedger117(facts:{arg1:string;company_id:unknown;id:unknown;previewKey:string;byteLength:number;completed:number;}):Effect;stepAttachmentScanAttempts118(facts:{code:string;completed:number;id:unknown;lease:string;}):Effect;stepAttachments119(facts:{id:unknown;}):Effect;stepAttachmentEvents120(facts:{arg1:string;company_id:unknown;id:unknown;arg4:"system:jobs";arg5:"scan.error";arg6:string;completed:number;}):Effect;stepAttachmentScanAttempts121(facts:{arg1:string;id:unknown;arg3:number;arg4:number;}):Effect;readAttachments122(facts:{attachmentId:string;}):Promise<{ source_state: AttachmentSourceState; } | null>;readAttachments123(facts:{attachmentId:string;}):Promise<{ source_state: string; legal_hold: number; } | null>;readAttachmentLinks124(facts:{now:number;attachmentId:string;}):Promise<{ active_links: number; closed_due_links: number; review_obligations: number; retain_until: number | null; } | null>;readLegalFilePreservations125(facts:{attachmentId:string;}):Promise<Record<string,unknown>|null>;readAttachmentSets126(facts:{now:number;arg2:number;}):Promise<AttachmentResult<{ company_id: string; target_id: string; }>>;readAttachmentLinks127(facts:{now:number;}):Promise<AttachmentResult<{ id: string; set_id: string; attachment_id: string | null; }>>;writeAttachmentEvents128(facts:{arg1:string;attachment_id:string | null;id:string;set_id:string;arg5:"system:jobs";arg6:"link.expired";arg7:"{}";now:number;}):Promise<AttachmentResult<Record<string, unknown>>>;writeUploadIntents129(facts:{now:number;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentSets130(facts:{now:number;arg2:number;}):Promise<AttachmentResult<{ id: string; company_id: string; }>>;writeAttachmentEvents131(facts:{arg1:string;company_id:string;id:string;now:number;}):Promise<AttachmentResult<Record<string, unknown>>>;readLeaveRequests132(facts:{target_id:string;company_id:string;}):Promise<{ id: string; } | null>;readAttachmentLinks133(facts:{company_id:string;target_id:string;now:number;}):Promise<AttachmentResult<{ id: string; set_id: string; attachment_id: string | null; }>>;writeAttachmentEvents134(facts:{arg1:string;company_id:string;attachment_id:string | null;id:string;set_id:string;now:number;}):Promise<AttachmentResult<Record<string, unknown>>>;writeUploadIntents135(facts:{now:number;company_id:string;target_id:string;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachmentSets136(facts:{now:number;company_id:string;target_id:string;arg4:number;}):Promise<AttachmentResult<{ id: string; company_id: string; }>>;writeAttachmentEvents137(facts:{arg1:string;company_id:string;id:string;now:number;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachments138(facts:{now:number;}):Promise<AttachmentResult<{ id: string; }>>;lock139(facts:{arg1:string;}):Promise<AttachmentResult<Record<string, unknown>>>;lock140(facts:Record<string,never>):Promise<AttachmentResult<Record<string, unknown>>>;readAttachments141(facts:{id:string;}):Promise<{ id: string; } | null>;readAttachments142(facts:{now:number;id:string;}):Promise<{ company_id: string; } | null>;writeAttachmentStorageLedger143(facts:{now:number;id:string;}):Promise<AttachmentResult<Record<string, unknown>>>;writeAttachmentEvents144(facts:{arg1:string;company_id:string;id:string;now:number;}):Promise<AttachmentResult<Record<string, unknown>>>;readAttachments145(facts:Record<string,never>):Promise<AttachmentResult<{ id: string; company_id:string; object_key: string; preview_object_key: string | null; }>>;readAttachments148(facts:{id:string;}):Promise<{ company_id: string; object_key: string; preview_object_key: string | null; source_state: string; legal_hold: number; } | null>;readAttachmentEvents149(facts:{id:string;}):Promise<Record<string, unknown> | null>;readAttachmentStorageLedger150(facts:{id:string;}):Promise<AttachmentResult<{ object_key: string; }>>;writeAttachmentEvents151(facts:{eventId:string;company_id:string;id:string;arg4:number;}):Promise<AttachmentResult<Record<string, unknown>>>;writeAttachmentEvents152(facts:{arg1:string;id:string;arg3:string;arg4:number;}):Promise<AttachmentResult<Record<string, unknown>>>;bindingGuard1(facts:{commandId:string;step:string;values:(string|number|null)[];}):Effect;bindingGuard2(facts:{commandId:string;step:string;values:(string|number|null)[];historicalLeave:boolean;}):Effect;bindingGuard3(facts:{commandId:string;step:string;values:(string|number|null)[];}):Effect;atomic<Result>(action:(unit:AttachmentServiceUnit<Effect>)=>Promise<Result>):Promise<Result>;uploadCleanupLock(key:string):Promise<unknown>;uploadObjectReferenced(companyId:string,key:string,excludedUploadId:string):Promise<unknown>;supportEvidenceEvent(id:string,key:string,actor:string,action:string,evidence:Record<string,unknown>,reason:string):Effect;batch(effects:Effect[]):Promise<AttachmentResult<unknown>[]>;authorizeTarget(actor:WorkContext,policy:AttachmentPolicy,id:string,operation:AttachmentOperation):Promise<AttachmentTargetScope>;supportEvidenceGuard(actor:WorkContext,id:string,version:number,key:string):Promise<{guards:Effect[]}>;replayCommand<Result=Record<string,unknown>>(actor:WorkContext,key:string,operation:string,id:string,input:unknown):Promise<Result|null>;executeCommand<Result>(actor:WorkContext,key:string,operation:string,input:unknown,id:string,version:number,result:Result,effects:Effect[],scope?:Pick<WorkContext,"companyId"|"branchId"|"departmentId">):Promise<Result>;};
export type AttachmentServicePorts={clock:Clock;identifiers:IdentifierSource;environment():AttachmentEnvironment;error(status:number,code:string,message:string):Error;hash(value:string):string;digest(bytes:ArrayBuffer):Promise<string>;isClientError(error:unknown):boolean;errorCode(error:unknown):string|undefined;errorStatus(error:unknown):number|undefined;reportFailure(event:Record<string,unknown>):void;uploadUrl(actor:WorkContext,token:string):string;
features:{attachmentUploadsFrozen(environment?:AttachmentEnvironment):boolean;attachmentsV2Enabled(environment?:AttachmentEnvironment):boolean;attachmentsV2ReadOnly(environment?:AttachmentEnvironment):boolean;requireAttachmentsV2():void;requireAttachmentsV2Read():void;};
providers:{detectedAttachmentMediaType(bytes:Uint8Array,declared:string):ReturnType<typeof detectMediaSignature>;attachmentScannerReadiness(environment:AttachmentEnvironment):Promise<{enabled:boolean;ready:boolean;engine:string|null;definitionUpdatedAt:number|null;error:string|null}>;attachmentPreviewReadiness(environment:AttachmentEnvironment):Promise<{ready:boolean;error:string|null}>;scanAttachmentBytes(bytes:Uint8Array,environment:AttachmentEnvironment):Promise<{clean:boolean;signature:string|null;engine:string;definitionUpdatedAt:number}>;sanitizedAttachmentPreview(bytes:Uint8Array,mediaType:string):Promise<{bytes:Uint8Array;mediaType:"image/webp"|"image/jpeg"}|null>;}};
export function createAttachmentService<Effect>(ports:AttachmentServicePorts){
 const {attachmentUploadsFrozen,attachmentsV2Enabled,attachmentsV2ReadOnly,requireAttachmentsV2,requireAttachmentsV2Read}=ports.features,{detectedAttachmentMediaType,attachmentScannerReadiness,attachmentPreviewReadiness,scanAttachmentBytes,sanitizedAttachmentPreview}=ports.providers;
 const {assertAttachmentMedia,attachmentPolicy,attachmentPolicyRegistry,attachmentRetentionSnapshot}=createAttachmentPolicy(ports);
 const {assertSanitizedEvidenceMetadata,requiresSanitizedEvidenceAttestation}=createFinanceAttachmentPolicy<Effect>(ports);
const brandPublicationTargets = new Set(["holding_brand","branch_brand","company_storefront_brand","pms_property","pms_room_type"]);
function isBrandPublicationTarget(targetType:string) { return brandPublicationTargets.has(targetType); }

async function atomic<Result>(db:AttachmentServiceUnit<Effect>,action:(unit:AttachmentServiceUnit<Effect>)=>Promise<Result>){return db.atomic(action);}

function stableId(prefix: string, actor: WorkContext, key: string) {
  return `${prefix}-${ports.hash(`${actor.companyId}\0${actor.userId}\0${key}`).slice(0,32)}`;
}

function safeJson(value: unknown) { return JSON.stringify(value).replaceAll("<", "\\u003c"); }

function safeUploadErrorCode(error: unknown) {
  const code = error && typeof error === "object" && "code" in error ? error.code : undefined;
  return typeof code === "string" && /^[A-Za-z0-9_]{1,80}$/.test(code) ? code : undefined;
}

type AttachmentServiceOptions={compatibilityAdapter?:boolean;requestId?:string};
function logUploadOperationFailure(options:AttachmentServiceOptions,operation:string,error:unknown){
  if(!options.requestId || ports.isClientError(error))return;
  const errorName=error instanceof Error&&/^[A-Za-z0-9_]{1,80}$/.test(error.name)?error.name:typeof error;
  ports.reportFailure({event:"attachment_upload_operation_failed",requestId:options.requestId,operation,errorName,errorCode:safeUploadErrorCode(error)});
}
function attachmentServiceEnvironment(options:AttachmentServiceOptions={}){
  return options.compatibilityAdapter&&!attachmentsV2Enabled()?{...ports.environment(),ERPX_ATTACHMENTS_V2:"true"}:ports.environment();
}
function requireAttachmentWrite(options:AttachmentServiceOptions={}){
  if(!options.compatibilityAdapter)return requireAttachmentsV2();
  if(attachmentsV2ReadOnly())throw ports.error(503,"ATTACHMENTS_READ_ONLY","Attachment intake is disabled during rollback. Existing clean bound evidence remains readable.");
}
function requireAttachmentRead(options:AttachmentServiceOptions={}){
  if(!options.compatibilityAdapter)return requireAttachmentsV2Read();
}

async function requireScannerReady(environment:AttachmentEnvironment=ports.environment()) {
  const [readiness,preview] = await Promise.all([attachmentScannerReadiness(environment),attachmentPreviewReadiness(environment)]);
  if (!readiness.ready || !readiness.enabled || !preview.ready)
    throw ports.error(503, "ATTACHMENT_SCANNER_UNAVAILABLE", "File intake is unavailable until the malware scanner and current signatures are ready.");
  return readiness;
}

async function setRow(db: AttachmentServiceUnit<Effect>, id: string, companyId: string) {
  const row = await db.readAttachmentSets1({id:id,companyId:companyId});
  if (!row) throw ports.error(404, "ATTACHMENT_SET_NOT_FOUND", "Attachment set not found.");
  return row;
}

/**
 * Leave approval and evidence intake share the leave_requests row as their
 * serialization boundary. Callers must invoke this inside the same database
 * transaction that mutates the attachment set/link/source. The second
 * authorization check is intentional: a requester can lose write access, or
 * the request can advance to a new version/final state, while bytes are being
 * selected and uploaded.
 */
async function lockLeaveAttachmentTargetRow(
  db: AttachmentServiceUnit<Effect>,
  actor: WorkContext,
  targetType: string,
  targetId: string,
) {
  if (targetType !== "leave_request") return false;
  if(actor.authority){
    // Match executeCommand's lock order (active context -> assignment -> Leave)
    // so a revocation that wins the race is observed, while one that follows
    // waits until this already-authorized mutation commits.
    const active=await db.readActiveWorkContexts2({sessionId:actor.authority.sessionId,companyId:actor.companyId});
    if(!active||active.user_id!==actor.userId||active.assignment_id!==actor.assignmentId||Number(active.assignment_version)!==actor.authority.assignmentVersion||Number(active.revision)!==actor.authority.revision||Number(active.expires_at)<=ports.clock.now())
      throw ports.error(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload before changing Leave evidence.");
    const assignment=await db.readAssignments3({assignmentId:actor.assignmentId,assignmentVersion:actor.authority.assignmentVersion,userId:actor.userId,role:actor.role,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId});
    if(!assignment)throw ports.error(409,"WORK_CONTEXT_CHANGED","Your assignment changed. Reload before changing Leave evidence.");
  }
  const locked = await db.readLeaveRequests4({targetId:targetId,companyId:actor.companyId});
  if (!locked)
    throw ports.error(404, "NOT_FOUND", "Leave request not found.");
  return true;
}

async function reauthorizeLockedLeaveAttachment(
  db: AttachmentServiceUnit<Effect>,
  actor: WorkContext,
  input: {
    targetType: string;
    targetId: string;
    purposeKey: string;
    componentKind: AttachmentComponentKind;
    expectedTargetVersion: number;
  },
  resolvedPolicy?: AttachmentPolicy,
  operation: "write" | "read" = "write",
): Promise<AttachmentTargetScope | null> {
  if (input.targetType !== "leave_request") return null;
  const scope = await db.authorizeTarget(
    actor,
    resolvedPolicy ?? attachmentPolicy(input.targetType, input.purposeKey, input.componentKind),
    input.targetId,
    operation,
  );
  if (scope.companyId !== actor.companyId)
    throw ports.error(403, "ATTACHMENT_CROSS_COMPANY", "Files cannot cross Companies.");
  if (operation === "write" && scope.version !== Number(input.expectedTargetVersion))
    throw ports.error(
      409,
      "ATTACHMENT_TARGET_CHANGED",
      "The leave request changed. Refresh before changing its evidence.",
    );
  return scope;
}

async function lockLeaveAttachmentMutation(
  db: AttachmentServiceUnit<Effect>,
  actor: WorkContext,
  input: Parameters<typeof reauthorizeLockedLeaveAttachment>[2],
  resolvedPolicy?: AttachmentPolicy,
) {
  if (!await lockLeaveAttachmentTargetRow(db, actor, input.targetType, input.targetId)) return null;
  return reauthorizeLockedLeaveAttachment(db, actor, input, resolvedPolicy, "write");
}

async function setLinks(db: AttachmentServiceUnit<Effect>, id: string, companyId: string) {
  return (await db.readAttachmentLinks5({id:id,companyId:companyId})).results
    .map((row) => ({...row,original_name:row.original_name??row.intent_original_name,media_type:row.media_type??row.intent_media_type}));
}

function linkDto(row: LinkRow): AttachmentLinkDto {
  return {
    id: row.id, sourceId: row.attachment_id, filename: row.original_name ?? "file", mediaType: row.media_type ?? "application/octet-stream",
    bytes: row.bytes ?? row.declared_bytes, sourceState: row.source_state, linkState: row.state, caption: row.caption,
    previewAvailable: !!row.preview_object_key, createdAt: Number(row.created_at), expiresAt: Number(row.expires_at), boundAt: row.bound_at === null ? null : Number(row.bound_at),
  };
}

async function setDto(db: AttachmentServiceUnit<Effect>, row: SetRow): Promise<AttachmentSetDto> {
  const sanitizationAttested=requiresSanitizedEvidenceAttestation(row.target_type)
    ? Boolean(await db.readAttachmentEvents6({id:row.id,company_id:row.company_id}))
    : undefined;
  return {
    id: row.id, targetType: row.target_type, targetId: row.target_id, componentKind: row.component_kind,
    componentRef: row.component_ref, purposeKey: row.purpose_key, expectedTargetVersion: Number(row.expected_target_version),
    policyVersion: Number(row.policy_version), classification: row.classification, requiredCount: Number(row.required_count),
    maxFiles: Number(row.max_files), state: row.state, createdAt: Number(row.created_at), updatedAt: Number(row.updated_at), sanitizationAttested,
    links: (await setLinks(db,row.id,row.company_id)).map(linkDto),
  };
}

async function listAttachmentSets(db: AttachmentServiceUnit<Effect>, actor: WorkContext, input: {targetType:string;targetId:string;purposeKey?:string;componentKind?:AttachmentComponentKind;expectedTargetVersion?:number}) {
  requireAttachmentsV2Read();
  if (!input.targetType || !input.targetId) throw ports.error(422,"ATTACHMENT_TARGET_REQUIRED","Choose a record before listing files.");
  const readOnly=attachmentsV2ReadOnly(),leaveReview=input.targetType==="leave_request";
  const rows = (await db.readAttachmentSets7({actor:actor,input:input,readOnly:readOnly,leaveReview:leaveReview})).results;
  for (const row of rows) await db.authorizeTarget(actor,attachmentPolicy(row.target_type,row.purpose_key,row.component_kind),row.target_id,"read");
  return Promise.all(rows.map((row) => setDto(db,row)));
}

async function recordedAttachmentSet(db:AttachmentServiceUnit<Effect>,actor:WorkContext,id:string){return await db.readAttachmentSets9({id,companyId:actor.companyId})??await db.readAttachmentSetAlias({id,companyId:actor.companyId,userId:actor.userId});}

async function createAttachmentSet(db: AttachmentServiceUnit<Effect>, actor: WorkContext, input: {targetType:string;targetId:string;componentKind:AttachmentComponentKind;componentRef?:string;purposeKey:string;expectedTargetVersion:number}, idempotencyKey: string, options:AttachmentServiceOptions={}) {
  requireAttachmentWrite(options);
  if(input.targetType==="field_service_visit")throw ports.error(503,"FIELD_FILES_NATIVE_REQUIRED","The native Field file service is required.");
  if(attachmentUploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  const policy = attachmentPolicy(input.targetType,input.purposeKey,input.componentKind);
  const retention=attachmentRetentionSnapshot(policy);
  const componentRef = input.componentRef?.trim() || null;
  const now=ports.clock.now(), id=stableId("attachment-set",actor,idempotencyKey);
  // Only an actual persisted command identity can recover old intake. A fresh
  // key still needs new work; draft reuse has its own persisted owner receipt.
  const recorded = await recordedAttachmentSet(db,actor,id);
  if(recorded){
    const readScope=await db.authorizeTarget(actor,policy,input.targetId,"read");
    if(readScope.companyId!==actor.companyId)throw ports.error(403,"ATTACHMENT_CROSS_COMPANY","Files cannot cross Companies.");
    return atomic(db,async tx=>{
      await tx.lock8({arg1:`attachment-set:${actor.companyId}:${actor.userId}:${idempotencyKey}`});
      const current=await recordedAttachmentSet(tx,actor,id);
      if(!current)throw ports.error(409,"ATTACHMENT_SET_CHANGED","The original file set is no longer available. Refresh before retrying.");
      await tx.authorizeTarget(actor,policy,input.targetId,"read");
      const compatibility=Boolean(await tx.readAttachmentEvents10({id:current.id,companyId:actor.companyId}));
      if(current.target_type!==input.targetType||current.target_id!==input.targetId||current.component_kind!==input.componentKind||current.component_ref!==componentRef||current.purpose_key!==input.purposeKey||Number(current.expected_target_version)!==input.expectedTargetVersion||compatibility!==Boolean(options.compatibilityAdapter))
        throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another attachment set.");
      return setDto(tx,current);
    });
  }
  const scope = await db.authorizeTarget(actor,policy,input.targetId,"write");
  if (scope.companyId !== actor.companyId) throw ports.error(403,"ATTACHMENT_CROSS_COMPANY","Files cannot cross Companies.");
  if (scope.version !== input.expectedTargetVersion) throw ports.error(409,"ATTACHMENT_TARGET_CHANGED","The record changed. Refresh before adding files.");
  if (componentRef && componentRef.length > 180) throw ports.error(422,"ATTACHMENT_COMPONENT_REF","The file context is too long.");
  // Current owner/package authorization precedes intake availability. A native
  // recorded-set replay above is an authorized read and opens no new intake.
  await requireScannerReady(attachmentServiceEnvironment(options));
  return atomic(db,async(tx)=>{
    await tx.lock8({arg1:`attachment-set:${actor.companyId}:${actor.userId}:${idempotencyKey}`});
    const lockedScope=await lockLeaveAttachmentMutation(tx,actor,input,policy)??scope;
    const prior=await recordedAttachmentSet(tx,actor,id);
    if(prior){
      const priorCompatibility=Boolean(await tx.readAttachmentEvents10({id:prior.id,companyId:actor.companyId}));
      if(prior.target_type!==input.targetType||prior.target_id!==input.targetId||prior.component_kind!==input.componentKind||prior.component_ref!==componentRef||prior.purpose_key!==input.purposeKey||Number(prior.expected_target_version)!==input.expectedTargetVersion||priorCompatibility!==Boolean(options.compatibilityAdapter))
        throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another attachment set.");
      return setDto(tx,prior);
    }
    const reusable=await tx.readAttachmentSets11({companyId:actor.companyId,targetType:input.targetType,targetId:input.targetId,componentKind:input.componentKind,componentRef:componentRef,purposeKey:input.purposeKey,expectedTargetVersion:input.expectedTargetVersion,userId:actor.userId,arg9:Boolean(options.compatibilityAdapter)});
    if(reusable){await tx.batch([tx.stepAttachmentSetAlias({id,companyId:actor.companyId,userId:actor.userId,setId:reusable.id,now})]);const saved=await tx.readAttachmentSetAlias({id,companyId:actor.companyId,userId:actor.userId});if(saved?.id!==reusable.id)throw ports.error(409,"ATTACHMENT_SET_CHANGED","The original reusable file set changed. Refresh before retrying.");return setDto(tx,saved);}
    await tx.batch([
      tx.stepAttachmentSets12({id:id,companyId:lockedScope.companyId,branchId:lockedScope.branchId,departmentId:lockedScope.departmentId,targetType:input.targetType,targetId:input.targetId,componentKind:input.componentKind,componentRef:componentRef,purposeKey:input.purposeKey,expectedTargetVersion:input.expectedTargetVersion,version:policy.version,classification:policy.classification,arg13:safeJson({classification:policy.classification,policyVersion:policy.version}),key:retention.key,versionValue15:retention.version,retainUntil:retention.retainUntil,destructionMode:retention.destructionMode,requiredCount:policy.requiredCount,maxFiles:policy.maxFiles,userId:actor.userId,now:now}),
      tx.stepAttachmentEvents13({arg1:`attachment-event-${ports.identifiers.next()}`,companyId:actor.companyId,id:id,userId:actor.userId,arg5:"set.created",arg6:safeJson({targetType:input.targetType,targetId:input.targetId,purposeKey:input.purposeKey,expectedTargetVersion:input.expectedTargetVersion,compatibilityAdapter:Boolean(options.compatibilityAdapter)}),now:now}),
    ]);
    return setDto(tx,(await setRow(tx,id,actor.companyId)));
  });
}

async function reserveAttachmentFile(db: AttachmentServiceUnit<Effect>, actor: WorkContext, setId: string, input: {filename:string;mediaType:string;bytes:number;caption?:string;supersedesLinkId?:string}, idempotencyKey: string, options:AttachmentServiceOptions={}) {
  requireAttachmentWrite(options); await db.legacyWriterGuard("set",setId); await requireScannerReady(attachmentServiceEnvironment(options));
  if(attachmentUploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  const set=await setRow(db,setId,actor.companyId),policy=attachmentPolicy(set.target_type,set.purpose_key,set.component_kind);
  const token=stableId("attachment-upload",actor,idempotencyKey);
  const recorded=await db.readUploadIntents16({token,companyId:actor.companyId,userId:actor.userId});
  if(recorded){
    const readScope=await db.authorizeTarget(actor,policy,set.target_id,"read");
    if(readScope.companyId!==actor.companyId)throw ports.error(403,"ATTACHMENT_CROSS_COMPANY","Files cannot cross Companies.");
    return atomic(db,async tx=>{
      await tx.lock15({arg1:`attachment-reserve:${actor.companyId}`});
      await tx.legacyWriterGuard("set",setId);
      const currentSet=await setRow(tx,setId,actor.companyId);
      await tx.authorizeTarget(actor,attachmentPolicy(currentSet.target_type,currentSet.purpose_key,currentSet.component_kind),currentSet.target_id,"read");
      const replay=await tx.readUploadIntents16({token,companyId:actor.companyId,userId:actor.userId});
      if(!replay)throw ports.error(409,"ATTACHMENT_RESERVATION_CHANGED","The original file reservation is no longer available. Refresh before retrying.");
      if(replay.attachment_set_id!==setId||replay.original_name!==input.filename.trim()||replay.media_type!==input.mediaType||Number(replay.declared_bytes)!==input.bytes||replay.caption!==(input.caption?.trim()||null)||replay.supersedes_link_id!==(input.supersedesLinkId??null))
        throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another file reservation.");
      return {uploadIntentId:token,attachmentLinkId:replay.attachment_link_id,uploadUrl:ports.uploadUrl(actor,token),expiresAt:Number(replay.expires_at)};
    });
  }
  const scope=await db.authorizeTarget(actor,policy,set.target_id,"write");
  if(set.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
  if(scope.version!==Number(set.expected_target_version))throw ports.error(409,"ATTACHMENT_TARGET_CHANGED","The record changed. Create a new file set for its current version.");
  assertAttachmentMedia(policy,input.mediaType,input.bytes);
  const filename=input.filename.trim(),caption=input.caption?.trim()||null;
  if(!filename||filename.length>255||caption&&caption.length>1000)throw ports.error(422,"ATTACHMENT_METADATA","Check the filename and caption.");
  assertSanitizedEvidenceMetadata(policy,filename,caption);
  if(input.supersedesLinkId){const prior=await db.readAttachmentLinks14({supersedesLinkId:input.supersedesLinkId,companyId:actor.companyId,target_type:set.target_type,target_id:set.target_id,purpose_key:set.purpose_key});if(!prior)throw ports.error(422,"ATTACHMENT_SUPERSEDES_INVALID","A correction may supersede only bound evidence on the same record and purpose.");}
  const now=ports.clock.now(),linkId=`attachment-link-${token.slice(token.lastIndexOf("-")+1)}`;
  return atomic(db,async(tx)=>{
    await tx.lock15({arg1:`attachment-reserve:${actor.companyId}`});
    await tx.legacyWriterGuard("set",setId);
    let currentSet=set;
    if(set.target_type==="leave_request"){
      // Read once to resolve the target, then again after the Leave row lock:
      // an approval that won the lock may have bound this set meanwhile.
      currentSet=await setRow(tx,setId,actor.companyId);
      await lockLeaveAttachmentMutation(tx,actor,{
        targetType:currentSet.target_type,
        targetId:currentSet.target_id,
        purposeKey:currentSet.purpose_key,
        componentKind:currentSet.component_kind,
        expectedTargetVersion:Number(currentSet.expected_target_version),
      });
      currentSet=await setRow(tx,setId,actor.companyId);
      if(currentSet.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
    }
    const replay=await tx.readUploadIntents16({token:token,companyId:actor.companyId,userId:actor.userId});
    if(replay){
      if(replay.attachment_set_id!==setId||replay.original_name!==filename||replay.media_type!==input.mediaType||Number(replay.declared_bytes)!==input.bytes
        ||replay.caption!==caption||replay.supersedes_link_id!==(input.supersedesLinkId??null))
        throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another file reservation.");
      return {uploadIntentId:token,attachmentLinkId:replay.attachment_link_id,uploadUrl:ports.uploadUrl(actor,token),expiresAt:Number(replay.expires_at)};
    }
    const count=Number(await tx.readAttachmentLinks17({setId:setId,companyId:actor.companyId}));
    if(count>=Number(currentSet.max_files))throw ports.error(422,"ATTACHMENT_LIMIT",`This file set accepts at most ${currentSet.max_files} files.`);
    const used=Number(await tx.readAttachmentStorageLedger18({companyId:actor.companyId}));
    const pending=Number(await tx.readUploadIntents19({companyId:actor.companyId,now:now}));
    const limit=Number((await tx.readBetaPolicy20({companyId:actor.companyId}))?.storage_bytes??Number.MAX_SAFE_INTEGER);
    if(used+pending+input.bytes>limit)throw ports.error(422,"STORAGE_QUOTA","Company file storage is full.");
    const reserveExpires=now+ATTACHMENT_RESERVATION_MS,draftExpires=reserveExpires,objectKey=`${actor.holdingId}/quarantine/${token}/original`;
    await tx.batch([
      tx.stepAttachmentLinks21({linkId:linkId,setId:setId,caption:caption,arg4:input.supersedesLinkId??null,userId:actor.userId,now:now,draftExpires:draftExpires}),
      tx.stepUploadIntents22({token:token,target_type:currentSet.target_type,target_id:currentSet.target_id,purpose_key:currentSet.purpose_key,filename:filename,mediaType:input.mediaType,bytes:input.bytes,objectKey:objectKey,arg9:"pending",userId:actor.userId,reserveExpires:reserveExpires,now:now,expected_target_version:currentSet.expected_target_version,setId:setId,linkId:linkId,companyId:actor.companyId}),
      tx.stepAttachmentLinks23({token:token,linkId:linkId}),
      tx.stepAttachmentEvents24({arg1:`attachment-event-${ports.identifiers.next()}`,companyId:actor.companyId,linkId:linkId,setId:setId,userId:actor.userId,arg6:"file.reserved",arg7:safeJson({filename,mediaType:input.mediaType,bytes:input.bytes}),now:now}),
    ]);
    return {uploadIntentId:token,attachmentLinkId:linkId,uploadUrl:ports.uploadUrl(actor,token),expiresAt:reserveExpires};
  });
}

async function completeAttachmentUpload(db: AttachmentServiceUnit<Effect>, files: AttachmentByteStore<AttachmentServiceUnit<Effect>>, actor: WorkContext, uploadIntentId: string, buffer: ArrayBuffer, options:AttachmentServiceOptions={}) {
  return completeAttachmentUploadInternal(db,files,actor,uploadIntentId,buffer,options,false);
}
async function completeAttachmentUploadInternal(db: AttachmentServiceUnit<Effect>, files: AttachmentByteStore<AttachmentServiceUnit<Effect>>, actor: WorkContext, uploadIntentId: string, buffer: ArrayBuffer, options:AttachmentServiceOptions, heldUploadUnit:boolean, onStored?:()=>void):Promise<{uploadIntentId:string;attachmentLinkId:string;sourceId:string;state:AttachmentSourceState;status:"scan_pending"}> {
  requireAttachmentWrite(options); if(!heldUploadUnit)await db.legacyWriterGuard("upload",uploadIntentId); await requireScannerReady(attachmentServiceEnvironment(options));
  if(attachmentUploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","File intake is briefly paused for storage reconciliation.");
  let intent:Record<string,unknown>|null;
  try {
    intent=await db.readUploadIntents25({uploadIntentId:uploadIntentId,userId:actor.userId,companyId:actor.companyId,arg4:ports.clock.now()});
  } catch(error) { logUploadOperationFailure(options,"load_intent",error); throw error; }
  if(!intent)throw ports.error(404,"UPLOAD_NOT_FOUND","Upload intent not found or expired.");
  // Replays may follow a completed owner command, but still require the
  // caller's current read grant. New bytes require the original live revision.
  if(intent.owner_type!=='leave_request'){
    const sourceSet=await setRow(db,String(intent.attachment_set_id),actor.companyId),replay=intent.status==='uploaded'||intent.status==='ready';
    const scope=await db.authorizeTarget(actor,attachmentPolicy(sourceSet.target_type,sourceSet.purpose_key,sourceSet.component_kind),sourceSet.target_id,replay?'read':'write');
    if(!replay&&(sourceSet.state!=='draft'||scope.version!==Number(intent.owner_version)||scope.version!==Number(sourceSet.expected_target_version)))throw ports.error(409,'ATTACHMENT_TARGET_CHANGED','The record changed after this file reservation. Create a file set for its current version.');
  }
  if(!heldUploadUnit){
    const expectedKey=String(intent.quarantine_key??intent.staging_key);let stored=false;
    try {
      return await db.legacyCleanupAtomic(actor.companyId,[expectedKey],async tx=>{
        if(intent.owner_type==="leave_request")await lockLeaveAttachmentTargetRow(tx,actor,"leave_request",String(intent.owner_id));
        await tx.legacyWriterGuard("upload",uploadIntentId);
        const current=await tx.readUploadIntents25({uploadIntentId,userId:actor.userId,companyId:actor.companyId,arg4:ports.clock.now()});
        if(!current||String(current.quarantine_key??current.staging_key)!==expectedKey)throw ports.error(409,"UPLOAD_CONFLICT","The upload reservation changed. Reload before retrying.");
        return completeAttachmentUploadInternal(tx,files,actor,uploadIntentId,buffer,options,true,()=>{stored=true;});
      });
    } catch(error) {
      if(stored)await db.legacyCleanupAtomic(actor.companyId,[expectedKey],async tx=>{
        await tx.legacyWriterGuard("upload",uploadIntentId);
        if(!await tx.uploadObjectReferenced(actor.companyId,expectedKey,uploadIntentId) && await tx.legacyCleanupObjectAllowed(actor.companyId,expectedKey))await files.delete(expectedKey,files.supportsTransactionDeletion?{unit:tx}:undefined);
      }).catch(()=>undefined);
      throw error;
    }
  }
  const declared=Number(intent.declared_bytes);
  if(buffer.byteLength<1||buffer.byteLength!==declared||buffer.byteLength>10*1024*1024)throw ports.error(422,"UPLOAD_SIZE","The uploaded bytes must exactly match the reserved size.");
  const bytes=new Uint8Array(buffer),detected=detectedAttachmentMediaType(bytes,String(intent.media_type));
  if(!detected||detected!==intent.media_type)throw ports.error(422,"UPLOAD_TYPE","The file contents do not match the selected type.");
  const checksum=await ports.digest(buffer),attachmentId=`attachment-${uploadIntentId}`,objectKey=String(intent.quarantine_key??intent.staging_key),now=ports.clock.now(),leaveIntent=intent.owner_type==="leave_request";
  if((intent.status==="uploaded"||intent.status==="ready")&&!leaveIntent){
    const existing=await db.readAttachments26({attachmentId:attachmentId,companyId:actor.companyId});
    if(!existing||existing.checksum_sha256!==checksum)throw ports.error(409,"UPLOAD_CONFLICT","This upload already contains different bytes.");
    return {uploadIntentId,attachmentLinkId:String(intent.attachment_link_id),sourceId:attachmentId,state:existing.source_state,status:"scan_pending" as const};
  }
  let stored: Awaited<ReturnType<AttachmentByteStore<AttachmentServiceUnit<Effect>>["put"]>> | null = null;
  try {
    stored=intent.status==="pending"
      ? await files.put(objectKey,buffer,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:detected,cacheControl:"private, no-store"},customMetadata:{sha256:checksum,state:"quarantined"}})
      : null;
  } catch (error) {
    logUploadOperationFailure(options,"object_put",error);
    throw error;
  }
  if(stored)onStored?.();
  if(intent.status==="pending"&&!stored){
    let prior;
    try { prior=await files.head(objectKey); }
    catch(error) { logUploadOperationFailure(options,"object_head",error); throw error; }
    if(prior?.customMetadata.sha256!==checksum)throw ports.error(409,"UPLOAD_CONFLICT","This upload key already contains different bytes.");
  }
  try {
    return await atomic(db,async(tx)=>{
      await tx.lock27({arg1:`attachment-upload:${actor.companyId}:${uploadIntentId}`});
      await tx.legacyWriterGuard("upload",uploadIntentId);
      const pendingSet=intent.owner_type==="leave_request"
        ? await setRow(tx,String(intent.attachment_set_id),actor.companyId)
        : null;
      if(pendingSet){
        if(pendingSet.target_type!==intent.owner_type||pendingSet.target_id!==intent.owner_id||pendingSet.purpose_key!==intent.purpose||Number(pendingSet.expected_target_version)!==Number(intent.owner_version))
          throw ports.error(409,"UPLOAD_CONFLICT","This upload reservation no longer matches its Leave evidence set.");
        // This must precede every intent/link/set row lock so it has the same
        // lock order as the Leave decision transaction.
        await lockLeaveAttachmentTargetRow(tx,actor,pendingSet.target_type,pendingSet.target_id);
      }
      const current=await tx.readUploadIntents28({uploadIntentId:uploadIntentId,userId:actor.userId,companyId:actor.companyId});
      if(!current||current.status==="pending"&&Number(current.expires_at)<=now)throw ports.error(404,"UPLOAD_NOT_FOUND","Upload intent not found or expired.");
      if(current.owner_type==="leave_request"){
        if(!pendingSet||String(current.attachment_set_id)!==pendingSet.id||pendingSet.target_type!==current.owner_type||pendingSet.target_id!==current.owner_id||pendingSet.purpose_key!==current.purpose||Number(pendingSet.expected_target_version)!==Number(current.owner_version))
          throw ports.error(409,"UPLOAD_CONFLICT","This upload reservation no longer matches its Leave evidence set.");
        await reauthorizeLockedLeaveAttachment(tx,actor,{
          targetType:pendingSet.target_type,
          targetId:pendingSet.target_id,
          purposeKey:pendingSet.purpose_key,
          componentKind:pendingSet.component_kind,
          expectedTargetVersion:Number(pendingSet.expected_target_version),
        },undefined,current.status==="uploaded"||current.status==="ready"?"read":"write");
      }else{
        const sourceSet=await setRow(tx,String(current.attachment_set_id),actor.companyId),replay=current.status==='uploaded'||current.status==='ready',scope=await tx.authorizeTarget(actor,attachmentPolicy(sourceSet.target_type,sourceSet.purpose_key,sourceSet.component_kind),sourceSet.target_id,replay?'read':'write');
        if(!replay&&(sourceSet.state!=='draft'||scope.version!==Number(current.owner_version)||scope.version!==Number(sourceSet.expected_target_version)))throw ports.error(409,'ATTACHMENT_TARGET_CHANGED','The record changed after this file reservation. Create a file set for its current version.');
      }
      if(current.status==="uploaded"||current.status==="ready"){
        const existing=await tx.readAttachments29({attachmentId:attachmentId,companyId:actor.companyId});
        if(!existing||existing.checksum_sha256!==checksum)throw ports.error(409,"UPLOAD_CONFLICT","This upload already contains different bytes.");
        if(current.owner_type==="leave_request"){
          const linked=await tx.readAttachmentLinks30({attachment_link_id:current.attachment_link_id,attachment_set_id:current.attachment_set_id,companyId:actor.companyId});
          if(!linked||linked.attachment_id!==attachmentId)throw ports.error(409,"UPLOAD_CONFLICT","The completed upload is no longer linked to its reserved Leave evidence slot.");
        }
        return {uploadIntentId,attachmentLinkId:String(current.attachment_link_id),sourceId:attachmentId,state:existing.source_state,status:"scan_pending" as const};
      }
      if(current.owner_type==="leave_request"){
        const lockedLink=await tx.readAttachmentLinks31({attachment_link_id:current.attachment_link_id,attachment_set_id:current.attachment_set_id,companyId:actor.companyId});
        if(!lockedLink||lockedLink.state!=="draft")throw ports.error(409,"ATTACHMENT_LINK_LOCKED","This file link is already bound or closed.");
        const lockedSet=await tx.readAttachmentSets32({attachment_set_id:current.attachment_set_id,companyId:actor.companyId});
        if(!lockedSet||lockedSet.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
      }
      if(current.status!=="pending")throw ports.error(409,"UPLOAD_CONFLICT","This upload intent is no longer writable.");
      await tx.batch([
        tx.stepAttachments33({attachmentId:attachmentId,uploadIntentId:uploadIntentId,owner_type:current.owner_type,owner_id:current.owner_id,purpose:current.purpose,objectKey:objectKey,revision:current.revision,original_name:current.original_name,media_type:current.media_type,byteLength:buffer.byteLength,checksum:checksum,userId:actor.userId,now:now,companyId:actor.companyId,detected:detected}),
        tx.stepUploadIntents34({byteLength:buffer.byteLength,checksum:checksum,detected:detected,objectKey:objectKey,now:now,uploadIntentId:uploadIntentId,companyId:actor.companyId}),
        tx.stepAttachmentLinks35({attachmentId:attachmentId,attachment_link_id:current.attachment_link_id,companyId:actor.companyId,ownerType:current.owner_type}),
        tx.stepAttachmentScanAttempts36({arg1:`attachment-scan-${ports.identifiers.next()}`,attachmentId:attachmentId,now:now}),
        tx.stepAttachmentStorageLedger37({arg1:`attachment-storage-${ports.identifiers.next()}`,companyId:actor.companyId,attachmentId:attachmentId,objectKey:objectKey,byteLength:buffer.byteLength,now:now}),
        tx.stepAttachmentEvents38({arg1:`attachment-event-${ports.identifiers.next()}`,companyId:actor.companyId,attachmentId:attachmentId,attachment_link_id:current.attachment_link_id,attachment_set_id:current.attachment_set_id,userId:actor.userId,arg7:"file.quarantined",arg8:safeJson({checksum,bytes:buffer.byteLength,detectedMediaType:detected}),now:now}),
      ]);
      return {uploadIntentId,attachmentLinkId:String(current.attachment_link_id),sourceId:attachmentId,state:"quarantined" as const,status:"scan_pending" as const};
    });
  } catch(error) {
    logUploadOperationFailure(options,"metadata_transaction",error);
    if(stored&&!heldUploadUnit){
      // The object store write precedes the database transaction. Serialize
      // cleanup with every retry and delete only when no immutable source won
      // the race and references this key.
      await db.legacyCleanupAtomic(actor.companyId,[objectKey],async cleanupTx=>{
        await cleanupTx.uploadCleanupLock(`attachment-upload:${actor.companyId}:${uploadIntentId}`);
        const referenced=await cleanupTx.uploadObjectReferenced(actor.companyId,objectKey,uploadIntentId);
        if(!referenced && await cleanupTx.legacyCleanupObjectAllowed(actor.companyId,objectKey))await files.delete(objectKey);
      }).catch(()=>undefined);
    }
    throw error;
  }
}

async function attachmentLinkStatus(db: AttachmentServiceUnit<Effect>, actor: WorkContext, linkId: string) {
  requireAttachmentsV2Read();
  const row=await db.readAttachmentLinks39({linkId:linkId,companyId:actor.companyId});
  if(!row)throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  if(attachmentsV2ReadOnly()&&(row.state!=="bound"||row.source_state!=="available"))throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  if(row.state==="draft"&&row.created_by!==actor.userId&&row.target_type!=="leave_request")throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  const policy=attachmentPolicy(String(row.target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind);
  const scope=await db.authorizeTarget(actor,policy,String(row.target_id),"read");
  const scan=row.attachment_id?await db.readAttachmentScanAttempts40({attachment_id:row.attachment_id}):null;
  const sourceState=(row.source_state??null) as AttachmentSourceState|null,linkState=row.state as AttachmentLinkState;
  await db.writeAttachmentAccessEvents41({arg1:`attachment-access-${ports.identifiers.next()}`,companyId:actor.companyId,attachment_id:row.attachment_id,linkId:linkId,userId:actor.userId,arg6:ports.clock.now()});
  return {linkId,setId:String(row.set_id),sourceId:row.attachment_id?String(row.attachment_id):null,sourceState,linkState,
    scan:{attempts:Number(scan?.attempt??0),lastResult:scan?.state??null,lastError:scan?.error_code??null,signature:scan?.scanner_signature??null,definitionUpdatedAt:scan?.scanner_definition_at??null,scannedAt:scan?.completed_at??null},
    previewAvailable:!!row.preview_object_key,bindable:sourceState==="available"&&linkState==="draft"&&(row.target_type==="leave_request"?scope.version>=Number(row.expected_target_version):scope.version===Number(row.expected_target_version)),
    expiresAt:Number(row.expires_at),boundAt:row.bound_at===null?null:Number(row.bound_at),withdrawnAt:row.withdrawn_at===null?null:Number(row.withdrawn_at)};
}

async function attachmentContent(db: AttachmentServiceUnit<Effect>, files: AttachmentByteStore<AttachmentServiceUnit<Effect>>, actor: WorkContext, linkId: string, variant: "original"|"preview", options:AttachmentServiceOptions={}) {
  requireAttachmentRead(options);
  const row=await db.readAttachmentLinks42({linkId:linkId,companyId:actor.companyId});
  if(!row)throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  try {
    if(attachmentsV2ReadOnly()&&(row.link_state!=="bound"||row.source_state!=="available"))throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    if(row.link_state==="draft"&&row.link_creator!==actor.userId&&row.target_type!=="leave_request")throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    if(row.link_state==="withdrawn"||row.link_state==="expired")throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    await db.authorizeTarget(actor,attachmentPolicy(String(row.target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind),String(row.target_id),"read");
    if(row.source_state!=="available")throw ports.error(423,"ATTACHMENT_NOT_AVAILABLE","The file is still quarantined or was rejected.");
    const key=variant==="preview"?row.preview_object_key:row.object_key;
    if(!key)throw ports.error(404,"ATTACHMENT_PREVIEW_NOT_FOUND","A safe preview is not available.");
    const object=await files.get(String(key));if(!object)throw ports.error(503,"FILE_UNAVAILABLE","The private file is temporarily unavailable.");
    await db.writeAttachmentAccessEvents43({arg1:`attachment-access-${ports.identifiers.next()}`,companyId:actor.companyId,attachment_id:row.attachment_id,linkId:linkId,userId:actor.userId,variant:variant,arg7:ports.clock.now()});
    return {object,mediaType:String(variant==="preview"?row.preview_media_type:row.media_type),filename:String(row.original_name),inline:variant==="preview"};
  } catch(error) {
    await db.writeAttachmentAccessEvents44({arg1:`attachment-access-${ports.identifiers.next()}`,companyId:actor.companyId,attachment_id:row.attachment_id,linkId:linkId,userId:actor.userId,variant:variant,arg7:ports.errorCode(error)??"ACCESS_DENIED",arg8:ports.clock.now()}).catch(()=>{});
    throw error;
  }
}

async function withdrawAttachmentLink(db: AttachmentServiceUnit<Effect>, actor: WorkContext, linkId: string, reason: string, idempotencyKey: string,options:AttachmentServiceOptions={}) {
  requireAttachmentWrite(options); await db.legacyWriterGuard("link",linkId); const text=reason.trim();if(text.length<3||text.length>1000)throw ports.error(422,"WITHDRAWAL_REASON_REQUIRED","Explain why this file is being withdrawn.");
  const row=await db.readAttachmentLinks45({linkId:linkId,companyId:actor.companyId});
  if(!row)throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
  if(row.state==="bound")throw ports.error(409,"BOUND_ATTACHMENT_IMMUTABLE","Bound evidence cannot be withdrawn. Add a correction instead.");
  if(row.created_by!==actor.userId)throw ports.error(403,"ATTACHMENT_WITHDRAW_FORBIDDEN","Only the person who staged this file may withdraw it.");
  await db.authorizeTarget(actor,attachmentPolicy(String(row.target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind),String(row.target_id),["marketing_asset","marketing_campaign","ecommerce_listing","messaging_room"].includes(String(row.target_type))?"withdraw":"write");
  return atomic(db,async tx=>{
    await tx.legacyWriterGuard("link",linkId);
    await lockLeaveAttachmentMutation(tx,actor,{
      targetType:String(row.target_type),
      targetId:String(row.target_id),
      purposeKey:String(row.purpose_key),
      componentKind:row.component_kind as AttachmentComponentKind,
      expectedTargetVersion:Number(row.expected_target_version),
    });
    const current=await tx.readAttachmentLinks46({linkId:linkId,companyId:actor.companyId});
    if(!current)throw ports.error(404,"ATTACHMENT_LINK_NOT_FOUND","File link not found.");
    if(current.target_type!==row.target_type||current.target_id!==row.target_id||current.purpose_key!==row.purpose_key||Number(current.expected_target_version)!==Number(row.expected_target_version))
      throw ports.error(409,"ATTACHMENT_LINK_LOCKED","This file link no longer belongs to the reviewed Leave evidence set.");
    if(current.target_type==="leave_request"&&current.set_state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
    if(current.state==="withdrawn"){
      if(current.withdrawn_by!==actor.userId||current.withdrawal_reason!==text)throw ports.error(409,"IDEMPOTENCY_CONFLICT","This file was withdrawn with a different command.");
      return {id:linkId,state:"withdrawn" as const};
    }
    if(current.state!=="draft")throw ports.error(409,"ATTACHMENT_LINK_LOCKED","This file link is closed.");
    const now=ports.clock.now();
    await tx.batch([
      tx.stepAttachmentLinks47({now:now,userId:actor.userId,text:text,linkId:linkId,companyId:actor.companyId}),
      tx.stepAttachmentEvents48({arg1:`attachment-withdraw-${ports.hash(`${actor.companyId}\0${linkId}\0${idempotencyKey}`).slice(0,32)}`,companyId:actor.companyId,attachment_id:current.attachment_id,linkId:linkId,set_id:current.set_id,userId:actor.userId,arg7:"link.withdrawn",arg8:safeJson({reason:text}),now:now}),
    ]);
    return {id:linkId,state:"withdrawn" as const};
  });
}

async function attestAttachmentSetSanitized(
  db: AttachmentServiceUnit<Effect>,
  actor: WorkContext,
  setId: string,
  input: { confirmed: boolean; note: string },
  idempotencyKey: string,
) {
  requireAttachmentsV2();
  const set=await setRow(db,setId,actor.companyId);
  if(!requiresSanitizedEvidenceAttestation(set.target_type))throw ports.error(422,"ATTACHMENT_ATTESTATION_NOT_APPLICABLE","This file purpose does not use the sanitized-system-evidence review.");
  if(input.confirmed!==true)throw ports.error(422,"ATTACHMENT_ATTESTATION_REQUIRED","Confirm the sanitized-evidence checklist before continuing.");
  const note=input.note.trim();
  if(note.length<10||note.length>1000)throw ports.error(422,"ATTACHMENT_ATTESTATION_NOTE","Record a concise review note of 10 to 1,000 characters.");
  const policy=attachmentPolicy(set.target_type,set.purpose_key,set.component_kind);
  await db.authorizeTarget(actor,policy,set.target_id,"write");
  if(set.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
  const eventId=stableId("attachment-attestation",actor,idempotencyKey),now=ports.clock.now();
  return atomic(db,async tx=>{
    await tx.lock49({arg1:`attachment-attestation:${actor.companyId}:${actor.userId}:${idempotencyKey}`});
    const prior=await tx.readAttachmentEvents50({eventId:eventId,companyId:actor.companyId,userId:actor.userId});
    if(prior?.set_id!==undefined&&prior.set_id!==setId)
      throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another sanitization attestation.");
    const current=await tx.readAttachmentSets51({setId:setId,companyId:actor.companyId});
    if(!current||current.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","This file set is already bound or closed.");
    const links=(await tx.readAttachmentLinks52({setId:setId,companyId:actor.companyId})).results;
    if(!links.length)throw ports.error(422,"ATTACHMENT_SET_EMPTY","Upload at least one sanitized review artifact before attesting.");
    if(links.some(link=>link.source_state!=="available"))throw ports.error(409,"ATTACHMENT_SCAN_PENDING","Every file must pass malware scanning before sanitization review.");
    for(const link of links)assertSanitizedEvidenceMetadata(policy,String(link.original_name),link.caption);
    const details=safeJson({
      note,
      checklist:{noPasswords:true,noPrivateKeys:true,noSecrets:true,noConfigurationDump:true,noDatabaseDump:true,sanitizedReviewArtifactOnly:true},
      manifest:links.map(link=>({linkId:link.id,sourceId:link.source_id,checksumSha256:link.checksum_sha256,mediaType:link.media_type,bytes:Number(link.bytes)})),
    });
    if(prior){
      if(prior.details_json!==details)
        throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used with a different sanitization attestation payload.");
      return setDto(tx,await setRow(tx,setId,actor.companyId));
    }
    await tx.batch([
      tx.stepAttachmentEvents53({eventId:eventId,companyId:actor.companyId,setId:setId,userId:actor.userId,arg5:"sanitization.attested",details:details,now:now}),
      tx.stepAttachmentSets54({now:now,setId:setId,companyId:actor.companyId}),
    ]);
    return setDto(tx,await setRow(tx,setId,actor.companyId));
  });
}

async function linkExistingAttachment(db: AttachmentServiceUnit<Effect>, actor: WorkContext, setId: string, input: {sourceLinkId:string;caption?:string}, idempotencyKey:string) {
  requireAttachmentsV2();await db.legacyWriterGuard("set",setId);await db.legacyWriterGuard("link",input.sourceLinkId);const target=await setRow(db,setId,actor.companyId),targetPolicy=attachmentPolicy(target.target_type,target.purpose_key,target.component_kind);
  await db.authorizeTarget(actor,targetPolicy,target.target_id,"write");
  if(target.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","The target file set is closed.");
  const source=await db.readAttachmentLinks55({sourceLinkId:input.sourceLinkId,companyId:actor.companyId});
  if(!source||source.company_id!==target.company_id)throw ports.error(404,"ATTACHMENT_SOURCE_NOT_FOUND","A reusable file was not found in this Company.");
  if(source.state!=="bound")throw ports.error(409,"ATTACHMENT_SOURCE_NOT_BOUND","Only immutable bound files can be reused on another record.");
  await db.authorizeTarget(actor,attachmentPolicy(String(source.target_type),String(source.purpose_key),source.component_kind as AttachmentComponentKind),String(source.target_id),"read");
  if(source.source_state!=="available")throw ports.error(409,"ATTACHMENT_SOURCE_UNAVAILABLE","Only clean available sources can be linked.");
  const classificationRank={public:0,internal:1,confidential:2,restricted:3} as const;
  if(classificationRank[target.classification]<classificationRank[String(source.classification) as keyof typeof classificationRank])throw ports.error(403,"ATTACHMENT_AUDIENCE_EXPANSION","A more sensitive source cannot be reused in a broader file audience.");
  if((targetPolicy.publication||targetPolicy.portalUpload)&&(source.target_type!==target.target_type||source.target_id!==target.target_id))throw ports.error(403,"ATTACHMENT_AUDIENCE_REUSE_FORBIDDEN","Public or portal-visible media must originate on the same authorized owner.");
  assertAttachmentMedia(targetPolicy,String(source.media_type),Number(source.bytes));
  const id=stableId("attachment-link",actor,idempotencyKey),caption=input.caption?.trim()||null;
  return atomic(db,async tx=>{
    await tx.lock56({arg1:`attachment-link:${actor.companyId}:${setId}`});
    await tx.legacyWriterGuard("set",setId);await tx.legacyWriterGuard("link",input.sourceLinkId);
    await lockLeaveAttachmentMutation(tx,actor,{
      targetType:target.target_type,
      targetId:target.target_id,
      purposeKey:target.purpose_key,
      componentKind:target.component_kind,
      expectedTargetVersion:Number(target.expected_target_version),
    },targetPolicy);
    const lockedTarget=await tx.readAttachmentSets57({setId:setId,companyId:actor.companyId});
    if(!lockedTarget)throw ports.error(404,"ATTACHMENT_SET_NOT_FOUND","Attachment set not found.");
    if(lockedTarget.target_type!==target.target_type||lockedTarget.target_id!==target.target_id||lockedTarget.purpose_key!==target.purpose_key||Number(lockedTarget.expected_target_version)!==Number(target.expected_target_version))
      throw ports.error(409,"ATTACHMENT_SET_LOCKED","The target file set changed while it was being reviewed.");
    if(lockedTarget.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","The target file set is closed.");
    const prior=await tx.readAttachmentLinks58({id:id,companyId:actor.companyId});
    if(prior){
      if(prior.set_id!==setId||prior.attachment_id!==source.attachment_id||prior.caption!==caption)throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another file link.");
      return {attachmentLinkId:id,sourceId:String(source.attachment_id)};
    }
    const existing=await tx.readAttachmentLinks59({setId:setId,attachment_id:source.attachment_id,companyId:actor.companyId});
    if(existing)return {attachmentLinkId:existing.id,sourceId:String(source.attachment_id)};
    const count=Number(await tx.readAttachmentLinks60({setId:setId,companyId:actor.companyId}));
    if(count>=Number(lockedTarget.max_files))throw ports.error(422,"ATTACHMENT_LIMIT","The target file set is full.");
    const crossRecord=source.target_type!==target.target_type||source.target_id!==target.target_id;
    const audit={
      sourceLinkId:input.sourceLinkId,
      crossRecord,
      source:{targetType:source.target_type,targetId:source.target_id,purposeKey:source.purpose_key,componentKind:source.component_kind,classification:source.classification,audienceJson:source.audience_json},
      destination:{targetType:target.target_type,targetId:target.target_id,purposeKey:target.purpose_key,componentKind:target.component_kind,classification:target.classification,audienceJson:target.audience_json},
      audienceExpansion:{occurred:crossRecord,classificationWidened:classificationRank[target.classification]<classificationRank[String(source.classification) as keyof typeof classificationRank],addedOwner:crossRecord?{targetType:target.target_type,targetId:target.target_id}:null},
    };
    const now=ports.clock.now();
    await tx.batch([
      tx.stepAttachmentLinks61({id:id,setId:setId,attachment_id:source.attachment_id,caption:caption,userId:actor.userId,now:now,arg7:now+ATTACHMENT_DRAFT_MS}),
      tx.stepAttachmentEvents62({arg1:`attachment-reuse-${ports.hash(`${actor.companyId}\0${idempotencyKey}`).slice(0,32)}`,companyId:actor.companyId,attachment_id:source.attachment_id,id:id,setId:setId,userId:actor.userId,arg7:"link.reused",arg8:safeJson(audit),now:now}),
    ]);
    return {attachmentLinkId:id,sourceId:String(source.attachment_id)};
  });
}

function validateBindingContract(contract:AttachmentBindingContract){
  if(!contract.targetType.trim()||!contract.bindingType.trim()||!contract.contexts.length)throw ports.error(422,"ATTACHMENT_BINDING_CONTRACT_INVALID","This domain action has no closed attachment binding contract.");
  const seen=new Set<string>();
  for(const context of contract.contexts){
    const purposeKey=context.purposeKey.trim(),componentRef=context.componentRef?.trim()??null,key=`${purposeKey}\0${context.componentKind}\0${componentRef??""}`;
    if(!purposeKey||seen.has(key)||(["action","event","line","message"].includes(context.componentKind)&&!componentRef))throw ports.error(422,"ATTACHMENT_BINDING_CONTRACT_INVALID","This domain action has an invalid or duplicate attachment context.");
    attachmentPolicy(contract.targetType,purposeKey,context.componentKind);
    seen.add(key);
  }
}

function bindingContextMatches(contract:AttachmentBindingContract,set:SetRow){
  return contract.contexts.some(context=>context.purposeKey===set.purpose_key&&context.componentKind===set.component_kind&&(context.componentRef?.trim()??null)===(set.component_ref?.trim()??null));
}

async function attachmentBindingServiceOptions(db:AttachmentServiceUnit<Effect>,actor:WorkContext,setIds:string[],options:AttachmentServiceOptions){
  if(options.compatibilityAdapter||attachmentsV2Enabled()||!setIds.length)return options;
  const marked=Number(await db.readAttachmentSets63({companyId:actor.companyId,setIds:setIds}));
  return marked===setIds.length?{compatibilityAdapter:true}:options;
}

async function prepareAttachmentBinding(db: AttachmentServiceUnit<Effect>, actor: WorkContext, input: {attachmentSetIds:string[];bindingContract:AttachmentBindingContract;targetId:string;expectedTargetVersion:number;commandId:string;bindingId:string},options:AttachmentServiceOptions={}): Promise<PreparedAttachmentBinding<Effect>> {
  const ids=[...new Set(input.attachmentSetIds)];if(!ids.length||ids.length>20)throw ports.error(422,"ATTACHMENT_SETS_REQUIRED","Choose one to twenty file sets.");
  options=await attachmentBindingServiceOptions(db,actor,ids,options);requireAttachmentWrite(options);
  if(attachmentUploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","File binding is briefly paused for storage reconciliation.");
  validateBindingContract(input.bindingContract);
  const {targetType,bindingType}=input.bindingContract;
  const manifest:AttachmentManifestItem[]=[],guards:Effect[]=[],steps:Effect[]=[],now=ports.clock.now();
  for(const [setIndex,id] of ids.entries()){
    await db.legacyWriterGuard("set",id);guards.push(db.legacyWriterEffect("set",id));
    const set=await setRow(db,id,actor.companyId);if(set.target_type!==targetType||set.target_id!==input.targetId)throw ports.error(422,"ATTACHMENT_SET_TARGET","A file set belongs to another record.");
    if(!bindingContextMatches(input.bindingContract,set))throw ports.error(422,"ATTACHMENT_BINDING_CONTEXT","Choose only files prepared for this exact domain action and component.");
    if(set.state!=="draft")throw ports.error(409,"ATTACHMENT_SET_LOCKED","A file set was already bound or closed.");
    const historicalLeave=set.target_type==="leave_request";
    if(historicalLeave?Number(set.expected_target_version)>input.expectedTargetVersion:Number(set.expected_target_version)!==input.expectedTargetVersion)
      throw ports.error(409,"ATTACHMENT_SET_STALE",historicalLeave?"This Leave file set was authorized against a newer request version.":"Create a file set for the current record version.");
    const policy=attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),scope=await db.authorizeTarget(actor,policy,set.target_id,"bind");
    if(scope.version!==input.expectedTargetVersion)throw ports.error(409,"ATTACHMENT_TARGET_CHANGED","The record changed before evidence was bound.");
    guards.push(attachmentTargetVersionGuard(db,input.commandId,set.target_type,set.target_id,actor,input.expectedTargetVersion,setIndex));
    if(set.target_type==="support_ticket")guards.push(...(await db.supportEvidenceGuard(actor,set.target_id,input.expectedTargetVersion,input.commandId)).guards);
    const links=await db.readAttachmentLinks64({id:id,companyId:actor.companyId});
    if(!links.results.length)throw ports.error(422,"ATTACHMENT_SET_EMPTY","Bind at least one available file.");
    if(links.results.some(link=>link.source_state!=="available"))throw ports.error(409,"ATTACHMENT_SCAN_PENDING","Every active file must pass malware scanning before this action.");
    if(links.results.length<Number(set.required_count))throw ports.error(422,"ATTACHMENT_REQUIRED",`At least ${set.required_count} ${set.purpose_key} file(s) are required.`);
    if(requiresSanitizedEvidenceAttestation(set.target_type)){
      const attested=await db.readAttachmentEvents65({id:id,companyId:actor.companyId});
      if(!attested)throw ports.error(422,"ATTACHMENT_SANITIZATION_ATTESTATION_REQUIRED","Review the final file list and attest that it contains only sanitized evidence before binding it.");
      guards.push(db.bindingGuard1({commandId:input.commandId,step:`attachment-attestation-${setIndex}`,values:[id,actor.companyId]}));
    }
    guards.push(db.bindingGuard2({commandId:input.commandId,step:`attachment-set-${setIndex}`,values:[id,actor.companyId,targetType,input.targetId,input.expectedTargetVersion,policy.version,set.purpose_key,set.component_kind,set.component_ref],historicalLeave:historicalLeave}));
    for(const [linkIndex,link] of links.results.entries()){
      guards.push(db.bindingGuard3({commandId:input.commandId,step:`attachment-link-${setIndex}-${linkIndex}`,values:[link.link_id,id,actor.companyId,link.checksum_sha256]}));
      manifest.push({linkId:link.link_id,sourceId:String(link.attachment_id),purposeKey:set.purpose_key,checksumSha256:String(link.checksum_sha256),mediaType:String(link.media_type),bytes:Number(link.bytes)});
      steps.push(db.stepAttachmentLinks66({now:now,bindingType:bindingType,bindingId:input.bindingId,link_id:link.link_id,companyId:actor.companyId}));
    }
    steps.push(db.stepAttachmentSets67({now:now,commandId:input.commandId,id:id,companyId:actor.companyId}));
    steps.push(db.stepAttachmentEvents68({arg1:`attachment-bind-${ports.hash(`${input.commandId}\0${id}`).slice(0,32)}`,companyId:actor.companyId,id:id,userId:actor.userId,arg5:"set.bound",arg6:safeJson({bindingType,bindingId:input.bindingId,manifest}),now:now}));
  }
  return {manifest,guards,steps};
}






async function bindRecordAttachmentSet(db:AttachmentServiceUnit<Effect>,actor:WorkContext,setId:string,expectedTargetVersion:number,commandId:string,options:AttachmentServiceOptions={}){
  requireAttachmentWrite(options);await db.legacyWriterGuard("set",setId);
  const set=await setRow(db,setId,actor.companyId),brandSource=isBrandPublicationTarget(set.target_type)&&set.component_kind==="publication";
  if(set.component_kind!=="record"&&!brandSource)throw ports.error(422,"ACTION_EVIDENCE_DOMAIN_BIND_REQUIRED","Action, line and event evidence must bind inside its domain command.");
  if(Number(set.expected_target_version)!==expectedTargetVersion)throw ports.error(409,"ATTACHMENT_SET_STALE","Create a record-document set for the current record version.");
  const operation=brandSource?"attachment.publication-source.bind":"attachment.record.bind",nextVersion=brandSource?expectedTargetVersion:expectedTargetVersion+1,prior=await db.replayCommand<{id:string;ownerVersion:number;attachmentSetId:string;attachmentLinkId:string;state:"available";publicationReviewRequired:boolean}>(actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(prior)return atomic(db,async tx=>{await tx.legacyWriterGuard("set",setId);const current=await tx.replayCommand<typeof prior>(actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(!current)throw ports.error(409,"ATTACHMENT_SET_CHANGED","The original file binding changed. Refresh before retrying.");await tx.authorizeTarget(actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,'read');return current;});
  await db.authorizeTarget(actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,"bind");
  return atomic(db,async tx=>{
  await tx.legacyWriterGuard("set",setId);
  const committed=await tx.replayCommand<typeof prior>(actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(committed){await tx.authorizeTarget(actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,"read");return committed;}
  const bindingType=brandSource?"publication_source":"record_document",prepared=await prepareAttachmentBinding(tx,actor,{attachmentSetIds:[setId],bindingContract:{targetType:set.target_type,bindingType,contexts:[{purposeKey:set.purpose_key,componentKind:set.component_kind,componentRef:set.component_ref}]},targetId:set.target_id,expectedTargetVersion,commandId,bindingId:set.target_id},options);
  return tx.executeCommand(actor,commandId,operation,{setId,expectedTargetVersion},set.target_id,nextVersion,{setId,version:nextVersion,manifest:prepared.manifest},[
    ...prepared.guards,...(brandSource?[]:[attachmentRecordVersionStep(tx,set.target_type,set.target_id,expectedTargetVersion,ports.clock.now(),{actor,commandId})]),...prepared.steps,
  ],{companyId:set.company_id,branchId:set.branch_id,departmentId:set.department_id});
  });
}

/** One-release adapter for callers that historically treated upload completion
 * as the owner mutation. New workflows must bind action evidence in their own
 * domain command instead. */
async function bindCompatibilityAttachmentSet(db:AttachmentServiceUnit<Effect>,actor:WorkContext,setId:string,expectedTargetVersion:number,commandId:string,options:AttachmentServiceOptions={}){
  requireAttachmentWrite(options);await db.legacyWriterGuard("set",setId);
  const set=await setRow(db,setId,actor.companyId),brandSource=isBrandPublicationTarget(set.target_type)&&set.component_kind==="publication";
  if(set.component_kind==="publication"&&!brandSource)throw ports.error(422,"PUBLICATION_SOURCE_UNSUPPORTED","This publication source requires an explicit domain binding workflow.");
  const operation=brandSource?"attachment.publication-source.compatibility-bind":"attachment.compatibility.bind",nextVersion=brandSource?expectedTargetVersion:expectedTargetVersion+1,prior=await db.replayCommand<{id:string;ownerVersion:number;attachmentSetId:string;attachmentLinkId:string;state:"available";publicationReviewRequired:boolean}>(actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(prior)return atomic(db,async tx=>{await tx.legacyWriterGuard("set",setId);const current=await tx.replayCommand<typeof prior>(actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(!current)throw ports.error(409,"ATTACHMENT_SET_CHANGED","The original file binding changed. Refresh before retrying.");await tx.authorizeTarget(actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,'read');return current;});
  await db.authorizeTarget(actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,"bind");
  return atomic(db,async tx=>{
  await tx.legacyWriterGuard("set",setId);
  const committed=await tx.replayCommand<typeof prior>(actor,commandId,operation,set.target_id,{setId,expectedTargetVersion});if(committed){await tx.authorizeTarget(actor,attachmentPolicy(set.target_type,set.purpose_key,set.component_kind),set.target_id,"read");return committed;}
  const bindingType=brandSource?"publication_source":"legacy_upload_adapter",prepared=await prepareAttachmentBinding(tx,actor,{attachmentSetIds:[setId],bindingContract:{targetType:set.target_type,bindingType,contexts:[{purposeKey:set.purpose_key,componentKind:set.component_kind,componentRef:set.component_ref}]},targetId:set.target_id,expectedTargetVersion,commandId,bindingId:set.target_id},options);
  const result={id:prepared.manifest[0]!.sourceId,ownerVersion:nextVersion,attachmentSetId:setId,attachmentLinkId:prepared.manifest[0]!.linkId,state:"available" as const,publicationReviewRequired:brandSource};
  const compatibilityEvents=set.target_type==="support_ticket"?[tx.supportEvidenceEvent(set.target_id,commandId,actor.userId,"attachment-add",{attachmentId:result.id,manifest:prepared.manifest},"Attached private support evidence")]:[];
  return tx.executeCommand(actor,commandId,operation,{setId,expectedTargetVersion},set.target_id,nextVersion,result,[
    ...prepared.guards,...(brandSource?[]:[attachmentRecordVersionStep(tx,set.target_type,set.target_id,expectedTargetVersion,ports.clock.now(),{actor,commandId})]),...prepared.steps,...compatibilityEvents,
  ],{companyId:set.company_id,branchId:set.branch_id,departmentId:set.department_id});
  });
}

function brandPublicationApprovalStep(db:AttachmentServiceUnit<Effect>,row:Record<string,unknown>,publicationId:string,actor:WorkContext,now:number):Effect|null{
  const targetType=String(row.source_target_type),targetId=String(row.source_target_id),purpose=String(row.purpose_key);
  if(targetType==="holding_brand"||targetType==="branch_brand"){
    const kind=targetType==="holding_brand"?"holding":"branch",purposeKey=purpose==="inverse"?"inverse":"primary";
    return db.stepOrganizationBrandProfiles71({kind:kind,targetId:targetId,arg3:kind==="branch"?actor.companyId:null,arg4:kind==="holding"?"holding":"branch",userId:actor.userId,now:now,publicationId:publicationId,purposeKey:purposeKey});
  }
  if(targetType==="company_storefront_brand")return db.stepStorefrontAppBrands72({userId:actor.userId,now:now,publicationId:publicationId,targetId:targetId});
  return null;
}

function brandPublicationRevocationStep(db:AttachmentServiceUnit<Effect>,row:Record<string,unknown>,publicationId:string):Effect|null{
  const targetType=String(row.source_target_type),targetId=String(row.source_target_id),purpose=String(row.purpose_key);
  if(targetType==="holding_brand"||targetType==="branch_brand"){
    const purposeKey=purpose==="inverse"?"inverse":"primary";
    return db.stepOrganizationBrandProfiles73({arg1:targetType==="holding_brand"?"holding":"branch",targetId:targetId,publicationId:publicationId,purposeKey:purposeKey});
  }
  if(targetType==="company_storefront_brand")return db.stepStorefrontAppBrands74({targetId:targetId,publicationId:publicationId});
  return null;
}

async function createAttachmentPublication(db:AttachmentServiceUnit<Effect>,files:AttachmentByteStore<AttachmentServiceUnit<Effect>>,actor:WorkContext,input:{sourceLinkId:string;targetType:string;targetId:string;altText:string;ordinal:number},idempotencyKey:string){
  requireAttachmentsV2();if(attachmentUploadsFrozen())throw ports.error(503,"ATTACHMENT_UPLOAD_FREEZE","File intake and publication are briefly paused for storage reconciliation.");const alt=input.altText.trim();if(!alt||alt.length>500||!Number.isSafeInteger(input.ordinal)||input.ordinal<0||input.ordinal>10_000)throw ports.error(422,"PUBLICATION_METADATA","Provide concise alt text and a valid display order.");
  const source=await db.readAttachmentLinks75({sourceLinkId:input.sourceLinkId,companyId:actor.companyId});
  if(!source||source.source_state!=="available"||!source.preview_object_key)throw ports.error(404,"PUBLICATION_SOURCE_NOT_FOUND","Choose an available image with a sanitized preview.");
  const policy=attachmentPolicy(String(source.source_target_type),String(source.purpose_key),source.component_kind as AttachmentComponentKind);if(!policy.publication)throw ports.error(422,"PUBLICATION_NOT_ALLOWED","This file purpose cannot be published.");
  await db.authorizeTarget(actor,policy,String(source.source_target_id),"publish");
  if(input.targetType!==source.source_target_type||input.targetId!==source.source_target_id)throw ports.error(422,"PUBLICATION_TARGET_MISMATCH","Published media must remain attached to its authorized source owner.");
  const id=stableId("media-publication",actor,idempotencyKey),existing=await db.readAttachmentPublications76({id:id,companyId:actor.companyId});
  if(existing){if(existing.source_link_id!==input.sourceLinkId||existing.target_type!==input.targetType||existing.target_id!==input.targetId||existing.alt_text!==alt||Number(existing.ordinal)!==input.ordinal)throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another publication.");return existing;}
  const original=await files.get(String(source.preview_object_key));if(!original)throw ports.error(503,"FILE_UNAVAILABLE","The sanitized media is temporarily unavailable.");const bytes=await original.arrayBuffer(),checksum=await ports.digest(bytes),now=ports.clock.now();
  return atomic(db,async tx=>{
    await tx.lock77({arg1:`attachment-publication:${actor.companyId}:${input.targetType}:${input.targetId}`});
    const replay=await tx.readAttachmentPublications78({id:id,companyId:actor.companyId});if(replay)return replay;
    const revision=Number(await tx.readAttachmentPublications79({companyId:actor.companyId,targetType:input.targetType,targetId:input.targetId})),key=`${actor.holdingId}/published/${id}/r${revision}`;
    await files.put(key,bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:String(source.preview_media_type),cacheControl:"no-store"},customMetadata:{sha256:checksum,sanitized:"true"}});
    await tx.batch([
      tx.stepAttachmentPublications80({id:id,companyId:actor.companyId,sourceLinkId:input.sourceLinkId,targetType:input.targetType,targetId:input.targetId,key:key,preview_media_type:source.preview_media_type,checksum:checksum,byteLength:bytes.byteLength,alt:alt,ordinal:input.ordinal,revision:revision,userId:actor.userId,now:now}),
      tx.stepAttachmentPublicationOrders81({companyId:actor.companyId,targetType:input.targetType,targetId:input.targetId,userId:actor.userId,now:now}),
      tx.stepAttachmentStorageLedger82({arg1:`attachment-storage-${ports.identifiers.next()}`,companyId:actor.companyId,attachment_id:source.attachment_id,key:key,byteLength:bytes.byteLength,now:now}),
      tx.stepAttachmentEvents83({arg1:`attachment-event-${ports.identifiers.next()}`,companyId:actor.companyId,attachment_id:source.attachment_id,sourceLinkId:input.sourceLinkId,userId:actor.userId,arg6:"publication.created",arg7:safeJson({publicationId:id,targetType:input.targetType,targetId:input.targetId,revision}),now:now}),
    ]);return tx.readAttachmentPublications84({id:id});
  });
}

async function publicationTargetPolicy(db:AttachmentServiceUnit<Effect>,actor:WorkContext,targetType:string,targetId:string,operation:"read"|"publish"){
  const policy=attachmentPolicyRegistry.find(item=>item.targetType===targetType&&item.publication);if(!policy)throw ports.error(422,"PUBLICATION_TARGET_UNSUPPORTED","This target does not publish media.");await db.authorizeTarget(actor,policy,targetId,operation);return policy;
}
async function listAttachmentPublications(db:AttachmentServiceUnit<Effect>,actor:WorkContext,targetType:string,targetId:string){
  requireAttachmentsV2Read();await publicationTargetPolicy(db,actor,targetType,targetId,"read");let canManage=!attachmentsV2ReadOnly();if(canManage)try{await publicationTargetPolicy(db,actor,targetType,targetId,"publish");}catch(error){if(ports.errorStatus(error)!==403)throw error;canManage=false;}const rows=(await db.readAttachmentPublications85({companyId:actor.companyId,targetType:targetType,targetId:targetId,canManage:canManage})).results,order=await db.readAttachmentPublicationOrders86({companyId:actor.companyId,targetType:targetType,targetId:targetId});return {targetType,targetId,orderVersion:Number(order?.version??1),items:rows};
}
async function revokeAttachmentPublication(db:AttachmentServiceUnit<Effect>,actor:WorkContext,id:string,reason:string,idempotencyKey:string){
  requireAttachmentsV2();
  const text=reason.trim();
  if(text.length<3||text.length>1000)throw ports.error(422,"PUBLICATION_REVOCATION_REASON","Explain why this media is being revoked.");
  const row=await db.readAttachmentPublications87({id:id,companyId:actor.companyId});
  if(!row)throw ports.error(404,"PUBLICATION_NOT_FOUND","Media publication not found.");
  const policy=attachmentPolicy(String(row.source_target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind);
  const marketing=["marketing_asset","marketing_campaign","ecommerce_listing"].includes(String(row.source_target_type)),eventId=`publication-revoke-${ports.hash(`${actor.companyId}\0${id}\0${idempotencyKey}`).slice(0,32)}`,facts={eventId,companyId:actor.companyId,userId:actor.userId,publicationId:id,reason:text};
  const recorded=marketing&&row.state==="revoked"?await db.readPublicationRevocation(facts):null;
  await db.authorizeTarget(actor,policy,String(row.source_target_id),marketing?recorded?"revoke-history":"revoke":"publish");
  if(!hasPermission(actor,"records.review")&&!hasPermission(actor,"commerce.approve"))throw ports.error(403,"PUBLICATION_APPROVER_REQUIRED","An authorized reviewer must revoke public media.");
  return atomic(db,async tx=>{
    const current=await tx.readAttachmentPublications88({id:id,companyId:actor.companyId});
    if(!current)throw ports.error(404,"PUBLICATION_NOT_FOUND","Media publication not found.");
    if(current.state==="revoked"){
      if(marketing){
        if(!await tx.readPublicationRevocation(facts))throw ports.error(409,"IDEMPOTENCY_CONFLICT","This publication was revoked by a different command.");
        await tx.authorizeTarget(actor,policy,String(row.source_target_id),"revoke-history");
      }
      if(current.revocation_reason!==text)throw ports.error(409,"IDEMPOTENCY_CONFLICT","This publication was revoked for a different reason.");
      return current;
    }
    if(current.state!=="approved")throw ports.error(409,"PUBLICATION_NOT_APPROVED","Only approved public media can be revoked.");
    const now=ports.clock.now();
    const pointerStep=brandPublicationRevocationStep(tx,row,id);
    await tx.batch([
      tx.stepAttachmentPublications89({userId:actor.userId,now:now,text:text,id:id,companyId:actor.companyId}),
      ...(pointerStep?[pointerStep]:[]),
      tx.stepAttachmentEvents90({arg1:`publication-revoke-${ports.hash(`${actor.companyId}\0${id}\0${idempotencyKey}`).slice(0,32)}`,companyId:actor.companyId,source_link_id:row.source_link_id,userId:actor.userId,arg5:"publication.revoked",arg6:safeJson({publicationId:id,reason:text}),now:now}),
    ]);
    return tx.readAttachmentPublications78({id:id,companyId:actor.companyId});
  });
}
async function reorderAttachmentPublications(db:AttachmentServiceUnit<Effect>,actor:WorkContext,input:{targetType:string;targetId:string;expectedVersion:number;items:Array<{id:string;ordinal:number}>},idempotencyKey:string){
  requireAttachmentsV2();
  await publicationTargetPolicy(db,actor,input.targetType,input.targetId,"publish");
  if(!Number.isSafeInteger(input.expectedVersion)||input.expectedVersion<1||!input.items.length||input.items.length>100
    ||new Set(input.items.map(item=>item.id)).size!==input.items.length
    ||input.items.some(item=>!Number.isSafeInteger(item.ordinal)||item.ordinal<0||item.ordinal>10_000))
    throw ports.error(422,"PUBLICATION_ORDER_INVALID","Provide each active publication once with a valid display order.");
  const request={targetType:input.targetType,targetId:input.targetId,expectedVersion:input.expectedVersion,items:[...input.items].sort((left,right)=>left.id.localeCompare(right.id))};
  const requestJson=safeJson(request),requestHash=ports.hash(requestJson),eventId=stableId("publication-order",actor,idempotencyKey);
  return atomic(db,async tx=>{
    await tx.lock92({arg1:`publication-order:${actor.companyId}:${actor.userId}:${idempotencyKey}`});
    const replay=await tx.readAttachmentEvents93({eventId:eventId,companyId:actor.companyId,userId:actor.userId});
    if(replay){
      const details=JSON.parse(replay.details_json) as {requestHash?:string;result?:{targetType:string;targetId:string;orderVersion:number}};
      if(details.requestHash!==requestHash||!details.result)
        throw ports.error(409,"IDEMPOTENCY_CONFLICT","This command key was already used for another public-media order.");
      return details.result;
    }
    const order=await tx.readAttachmentPublicationOrders94({companyId:actor.companyId,targetType:input.targetType,targetId:input.targetId});
    const current=Number(order?.version??1);
    if(current!==input.expectedVersion)throw ports.error(409,"PUBLICATION_ORDER_CHANGED","Public media order changed. Refresh and retry.");
    const active=(await tx.readAttachmentPublications95({companyId:actor.companyId,targetType:input.targetType,targetId:input.targetId})).results.map(row=>row.id).sort();
    const submitted=input.items.map(item=>item.id).sort();
    if(active.length!==submitted.length||active.some((id,index)=>id!==submitted[index]))
      throw ports.error(409,"PUBLICATION_SET_CHANGED","Public media changed. Refresh and retry.");
    const now=ports.clock.now(),result={targetType:input.targetType,targetId:input.targetId,orderVersion:current+1};
    await tx.batch([
      ...input.items.map(item=>tx.stepAttachmentPublications96({ordinal:item.ordinal,id:item.id,companyId:actor.companyId})),
      tx.stepAttachmentPublicationOrders97({companyId:actor.companyId,targetType:input.targetType,targetId:input.targetId,userId:actor.userId,now:now,expectedVersion:input.expectedVersion}),
      tx.stepAttachmentEvents98({eventId:eventId,companyId:actor.companyId,userId:actor.userId,arg4:"publication.reordered",arg5:safeJson({requestHash,request,result}),now:now}),
    ]);
    return result;
  });
}

async function approveAttachmentPublication(db:AttachmentServiceUnit<Effect>,actor:WorkContext,id:string,idempotencyKey:string){
  requireAttachmentsV2();const row=await db.readAttachmentPublications99({id:id,companyId:actor.companyId});
  if(!row)throw ports.error(404,"PUBLICATION_NOT_FOUND","Media publication not found.");
  if(row.created_by===actor.userId)throw ports.error(403,"INDEPENDENT_PUBLICATION_REVIEW","Another authorized person must approve public media.");
  if(!hasPermission(actor,"records.review")&&!hasPermission(actor,"commerce.approve"))throw ports.error(403,"PUBLICATION_APPROVER_REQUIRED","An authorized reviewer must approve public media.");
  const policy=attachmentPolicy(String(row.source_target_type),String(row.purpose_key),row.component_kind as AttachmentComponentKind);await db.authorizeTarget(actor,policy,String(row.source_target_id),"publish");if(row.state==="approved")return row;if(row.state!=="draft")throw ports.error(409,"PUBLICATION_LOCKED","This publication can no longer be approved.");const now=ports.clock.now();
  return atomic(db,async tx=>{const current=await tx.readAttachmentPublications100({id:id,companyId:actor.companyId});if(current?.state==="approved")return tx.readAttachmentPublications78({id:id,companyId:actor.companyId});if(current?.state!=="draft")throw ports.error(409,"PUBLICATION_LOCKED","This publication can no longer be approved.");const pointerStep=brandPublicationApprovalStep(tx,row,id,actor,now);await tx.batch([
    tx.stepAttachmentPublications102({userId:actor.userId,now:now,id:id,companyId:actor.companyId}),
    ...(pointerStep?[pointerStep]:[]),
    tx.stepAttachmentEvents103({arg1:`publication-approval-${ports.hash(`${actor.companyId}\0${id}\0${idempotencyKey}`).slice(0,32)}`,companyId:actor.companyId,source_link_id:row.source_link_id,userId:actor.userId,arg5:"publication.approved",arg6:safeJson({publicationId:id,revision:row.revision}),now:now}),
  ]);return tx.readAttachmentPublications78({id:id,companyId:actor.companyId});});
}

async function publicAttachmentPublication(db:AttachmentServiceUnit<Effect>,files:AttachmentByteStore<AttachmentServiceUnit<Effect>>,id:string){
  const row=await db.readAttachmentPublications105({id:id});
  if(!row)throw ports.error(404,"PUBLICATION_NOT_FOUND","Published media not found.");const object=await files.get(row.derivative_object_key);if(!object)throw ports.error(503,"FILE_UNAVAILABLE","Published media is temporarily unavailable.");return {row,object};
}

async function drainAttachmentScans(db:AttachmentServiceUnit<Effect>,files:AttachmentByteStore<AttachmentServiceUnit<Effect>>,attachmentId?:string,options:{offlineMigration?:boolean}={}){
  if(options.offlineMigration&&!attachmentUploadsFrozen())throw new Error("OFFLINE_ATTACHMENT_SCAN_REQUIRES_UPLOAD_FREEZE");
  const scannerEnvironment=!attachmentsV2Enabled()?{...ports.environment(),ERPX_ATTACHMENTS_V2:"true"}:ports.environment();
  await requireScannerReady(scannerEnvironment);const now=ports.clock.now(),lease=`attachment-scan-lease-${ports.identifiers.next()}`;
  const job=await db.readAttachmentScanAttempts106({now:now,arg2:now+60_000,lease:lease,arg4:attachmentId??null,arg5:attachmentId??null});if(!job)return false;
  const source=await db.readAttachments107({attachment_id:job.attachment_id});if(!source)throw new Error("ATTACHMENT_SCAN_SOURCE_MISSING");
  const guard=()=>db.legacyScanGuard({sourceId:String(source.id),jobId:String(job.id),lease});
  await db.batch([guard(),db.legacyScanMark({sourceId:String(source.id)})]);
  try{
    const stored=await files.get(String(source.object_key));if(!stored)throw new Error("ATTACHMENT_OBJECT_MISSING");const originalBuffer=await stored.arrayBuffer(),bytes=new Uint8Array(originalBuffer);
    if(bytes.byteLength>10*1024*1024)throw new Error("ATTACHMENT_SCAN_OVERSIZE");
    if(await ports.digest(originalBuffer)!==source.checksum_sha256)throw new Error("ATTACHMENT_CHECKSUM_MISMATCH");
    const detected=detectedAttachmentMediaType(bytes,String(source.media_type));
    if(!detected||detected!==source.media_type)throw new Error("ATTACHMENT_MEDIA_MISMATCH");
    const scanned=await scanAttachmentBytes(bytes,scannerEnvironment);
    if(!scanned.clean){await db.batch([
      guard(),
      db.stepAttachmentScanAttempts109({engine:scanned.engine,signature:scanned.signature,definitionUpdatedAt:scanned.definitionUpdatedAt,arg4:ports.clock.now(),id:job.id,lease:lease}),
      db.stepAttachments110({detected:detected,arg2:ports.clock.now(),definitionUpdatedAt:scanned.definitionUpdatedAt,id:source.id}),
      db.stepAttachmentEvents111({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:source.company_id,id:source.id,arg4:"system:jobs",arg5:"scan.malware_rejected",arg6:safeJson({signature:scanned.signature,engine:scanned.engine}),arg7:ports.clock.now()}),
    ]);return true;}
    const preview=await sanitizedAttachmentPreview(bytes,String(source.media_type)),previewBuffer=preview?preview.bytes.slice().buffer:null,previewKey=preview&&previewBuffer?`${String(source.object_key).replace(/\/original$/,"")}/preview-${await ports.digest(previewBuffer)}.${preview.mediaType==="image/webp"?"webp":"jpg"}`:null;
    if(preview&&previewKey)await files.put(previewKey,preview.bytes,{onlyIf:{etagDoesNotMatch:"*"},httpMetadata:{contentType:preview.mediaType,cacheControl:"private, no-store"},customMetadata:{sourceSha256:String(source.checksum_sha256),sanitized:"true"}});
    const previewChecksum=previewBuffer?await ports.digest(previewBuffer):null,completed=ports.clock.now(),statements:Effect[]=[
      guard(),
      db.stepAttachmentScanAttempts112({engine:scanned.engine,definitionUpdatedAt:scanned.definitionUpdatedAt,completed:completed,id:job.id,lease:lease}),
      db.stepAttachments113({detected:detected,completed:completed,definitionUpdatedAt:scanned.definitionUpdatedAt,previewKey:previewKey,arg5:preview?.mediaType??null,previewChecksum:previewChecksum,arg7:preview?.bytes.byteLength??null,id:source.id}),
      db.stepUploadIntents114({completed:completed,upload_id:source.upload_id}),
      db.stepAttachmentLinks115({arg1:completed+ATTACHMENT_DRAFT_MS,upload_id:source.upload_id}),
      db.stepAttachmentEvents116({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:source.company_id,id:source.id,arg4:"system:jobs",arg5:"scan.available",arg6:safeJson({engine:scanned.engine,definitionUpdatedAt:scanned.definitionUpdatedAt,preview:!!preview}),completed:completed}),
    ];if(preview&&previewKey)statements.push(db.stepAttachmentStorageLedger117({arg1:`attachment-storage-${ports.identifiers.next()}`,company_id:source.company_id,id:source.id,previewKey:previewKey,byteLength:preview.bytes.byteLength,completed:completed}));
    await db.batch(statements);return true;
  }catch(error){const attempt=Number(job.attempt),completed=ports.clock.now(),code=(error instanceof Error?error.message:"SCAN_ERROR").slice(0,160),steps=[
    guard(),
    db.stepAttachmentScanAttempts118({code:code,completed:completed,id:job.id,lease:lease}),
    db.stepAttachments119({id:source.id}),
    db.stepAttachmentEvents120({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:source.company_id,id:source.id,arg4:"system:jobs",arg5:"scan.error",arg6:safeJson({attempt,code}),completed:completed}),
  ];if(attempt<6)steps.push(db.stepAttachmentScanAttempts121({arg1:`attachment-scan-${ports.identifiers.next()}`,id:source.id,arg3:attempt+1,arg4:completed+Math.min(60_000,1000*2**attempt)}));await db.batch(steps);return true;}
}

async function scanAttachmentNow(db:AttachmentServiceUnit<Effect>,files:AttachmentByteStore<AttachmentServiceUnit<Effect>>,attachmentId:string){
  if(!attachmentId)throw ports.error(422,"ATTACHMENT_SOURCE_REQUIRED","Choose a quarantined file source.");
  await drainAttachmentScans(db,files,attachmentId);
  const row=await db.readAttachments122({attachmentId:attachmentId});
  if(!row)throw ports.error(404,"ATTACHMENT_SOURCE_NOT_FOUND","File source not found.");
  return row.source_state;
}

async function attachmentDestructionEligibility(db:AttachmentServiceUnit<Effect>,attachmentId:string,now=ports.clock.now()){
  const source=await db.readAttachments123({attachmentId:attachmentId});
  if(!source)return {eligible:false,reason:"source_missing",retainUntil:null};
  if(Number(source.legal_hold)!==0)return {eligible:false,reason:"legal_hold",retainUntil:null};
  if(await activeAttachmentLegalPreservation(db,attachmentId))return {eligible:false,reason:"legal_preservation",retainUntil:null};
  if(source.source_state!=="available")return {eligible:false,reason:"source_not_available",retainUntil:null};
  const obligations=await db.readAttachmentLinks124({now:now,attachmentId:attachmentId});
  if(Number(obligations?.active_links??0)>0)return {eligible:false,reason:"active_link",retainUntil:obligations?.retain_until??null};
  if(Number(obligations?.closed_due_links??0)<1)return {eligible:false,reason:"draft_retention",retainUntil:obligations?.retain_until??null};
  if(Number(obligations?.review_obligations??0)>0)return {eligible:false,reason:"retention_review",retainUntil:obligations?.retain_until??null};
  const retainUntil=obligations?.retain_until===null||obligations?.retain_until===undefined?null:Number(obligations.retain_until);
  if(retainUntil!==null&&retainUntil>now)return {eligible:false,reason:"retain_until",retainUntil};
  return {eligible:true,reason:"eligible",retainUntil};
}

async function activeAttachmentLegalPreservation(db:AttachmentServiceUnit<Effect>,attachmentId:string){
  return Boolean(await db.readLegalFilePreservations125({attachmentId:attachmentId}));
}

async function cleanupAttachmentDrafts(db:AttachmentServiceUnit<Effect>,files:AttachmentByteStore<AttachmentServiceUnit<Effect>>){
  if(!attachmentsV2Enabled()||attachmentUploadsFrozen())return {expired:0,expiredSets:0,destroyed:0,bytesDeleted:0,deletionFailures:0};const now=ports.clock.now();
  // Leave approval uses the owner row as the manifest serialization boundary.
  // Exclude Leave from bulk expiry and process each request while holding that
  // same row lock, so cleanup cannot invalidate a manifest between its guards
  // and bind steps.
  const leaveTargets=(await db.readAttachmentSets126({now:now,arg2:now-ATTACHMENT_DRAFT_MS})).results;
  const expired=await atomic(db,async tx=>{
    const links=(await tx.readAttachmentLinks127({now:now})).results;
    for(const link of links)await tx.writeAttachmentEvents128({arg1:`attachment-event-${ports.identifiers.next()}`,attachment_id:link.attachment_id,id:link.id,set_id:link.set_id,arg5:"system:jobs",arg6:"link.expired",arg7:"{}",now:now});
    return links;
  });
  await atomic(db,async tx=>{await tx.writeUploadIntents129({now:now});});
  const expiredSets=await atomic(db,async tx=>{
    const sets=(await tx.readAttachmentSets130({now:now,arg2:now-ATTACHMENT_DRAFT_MS})).results;
    for(const set of sets)await tx.writeAttachmentEvents131({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:set.company_id,id:set.id,now:now});
    return sets;
  });
  for(const target of leaveTargets){
    const result=await atomic(db,async tx=>{
      const owner=await tx.readLeaveRequests132({target_id:target.target_id,company_id:target.company_id});
      if(!owner)return {links:[] as {id:string;set_id:string;attachment_id:string|null}[],sets:[] as {id:string;company_id:string}[]};
      const links=(await tx.readAttachmentLinks133({company_id:target.company_id,target_id:target.target_id,now:now})).results;
      for(const link of links)await tx.writeAttachmentEvents134({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:target.company_id,attachment_id:link.attachment_id,id:link.id,set_id:link.set_id,now:now});
      await tx.writeUploadIntents135({now:now,company_id:target.company_id,target_id:target.target_id});
      const sets=(await tx.readAttachmentSets136({now:now,company_id:target.company_id,target_id:target.target_id,arg4:now-ATTACHMENT_DRAFT_MS})).results;
      for(const set of sets)await tx.writeAttachmentEvents137({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:set.company_id,id:set.id,now:now});
      return {links,sets};
    });
    expired.push(...result.links);expiredSets.push(...result.sets);
  }
  const sources=(await db.readAttachments138({now:now})).results;
  let destroyed=0;
  for(const source of sources){
    const sealed=await atomic(db,async tx=>{
      await tx.lock139({arg1:`attachment-destroy:${source.id}`});
      await tx.lock140({});
      if(!await tx.legacyCleanupSourceAllowed(source.id))return false;
      const locked=await tx.readAttachments141({id:source.id});
      if(!locked||!(await attachmentDestructionEligibility(tx,source.id,now)).eligible)return false;
      const transitioned=await tx.readAttachments142({now:now,id:source.id});
      if(!transitioned)return false;
      await tx.writeAttachmentStorageLedger143({now:now,id:source.id});
      await tx.writeAttachmentEvents144({arg1:`attachment-event-${ports.identifiers.next()}`,company_id:transitioned.company_id,id:source.id,now:now});
      return true;
    });
    if(sealed)destroyed++;
  }
  const pendingDeletion=(await db.readAttachments145({})).results;
  let bytesDeleted=0,deletionFailures=0;
  for(const source of pendingDeletion){
    try{
      const initialLedger=(await db.readAttachmentStorageLedger150({id:source.id})).results.map(row=>row.object_key);
      const expectedKeys=[...new Set([source.object_key,...(source.preview_object_key?[source.preview_object_key]:[]),...initialLedger])].sort();
      const deleted=await db.legacyCleanupAtomic(source.company_id,expectedKeys,async tx=>{
        await tx.lock139({arg1:`attachment-destroy:${source.id}`});
        await tx.lock140({});
        if(!await tx.legacyCleanupSourceAllowed(source.id))return false;
        const locked=await tx.readAttachments148({id:source.id});
        if(!locked||locked.source_state!=="destroyed"||Number(locked.legal_hold)!==0||await activeAttachmentLegalPreservation(tx,source.id))return false;
        if(await tx.readAttachmentEvents149({id:source.id}))return false;
        const ledgerKeys=(await tx.readAttachmentStorageLedger150({id:source.id})).results.map(row=>row.object_key);
        const keys=[...new Set([locked.object_key,...(locked.preview_object_key?[locked.preview_object_key]:[]),...ledgerKeys])].sort();
        if(locked.company_id!==source.company_id || keys.length!==expectedKeys.length || keys.some((key,index)=>key!==expectedKeys[index]))return false;
        for(const key of keys)if(!await tx.legacyCleanupObjectAllowed(locked.company_id,key))return false;
        await files.delete(keys,files.supportsTransactionDeletion?{unit:tx}:undefined);
        const eventId=`attachment-bytes-deleted-${ports.hash(source.id).slice(0,32)}`;
        await tx.writeAttachmentEvents151({eventId:eventId,company_id:locked.company_id,id:source.id,arg4:ports.clock.now()});
        return true;
      });
      if(deleted)bytesDeleted++;
    }catch(error){
      deletionFailures++;
      const code=(error instanceof Error?error.message:"ATTACHMENT_BYTE_DELETE_FAILED").slice(0,160);
      await atomic(db,async tx=>{if(await tx.legacyCleanupSourceAllowed(source.id))await tx.writeAttachmentEvents152({arg1:`attachment-event-${ports.identifiers.next()}`,id:source.id,arg3:safeJson({code}),arg4:ports.clock.now()});});
    }
  }
  return {expired:expired.length,expiredSets:expiredSets.length,destroyed,bytesDeleted,deletionFailures};
}

function attachmentTargetVersionGuard(db:AttachmentServiceUnit<Effect>,commandId:string,targetType:string,targetId:string,actor:WorkContext,version:number,index:number){return db.ownerVersionGuard(commandId,targetType,targetId,actor,version,index);}
function attachmentRecordVersionStep(db:AttachmentServiceUnit<Effect>,targetType:string,targetId:string,expectedVersion:number,now=ports.clock.now(),context?:{actor:WorkContext;commandId:string}){return context?db.ownerVersionStep(targetType,targetId,expectedVersion,now,context):db.ownerVersionStep(targetType,targetId,expectedVersion,now);}

return {listAttachmentSets,createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,attachmentLinkStatus,attachmentContent,withdrawAttachmentLink,attestAttachmentSetSanitized,linkExistingAttachment,prepareAttachmentBinding,attachmentRecordVersionStep,bindRecordAttachmentSet,bindCompatibilityAttachmentSet,createAttachmentPublication,listAttachmentPublications,revokeAttachmentPublication,reorderAttachmentPublications,approveAttachmentPublication,publicAttachmentPublication,drainAttachmentScans,scanAttachmentNow,attachmentDestructionEligibility,cleanupAttachmentDrafts};}
