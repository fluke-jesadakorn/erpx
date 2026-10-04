import 'server-only';
import type {WorkContext} from './authorization';
import type {SqlDatabase} from './runtime/types';
import type {AttachmentBindingContract} from './attachments/types';
import {prepareAttachmentBinding} from './attachments/service';
import {attachmentPolicyRegistry} from './attachments/policy';
import {ApiError} from './http';

/** Closed aliases for the same native aggregate, consumed by its own command. */
export async function prepareBusinessEvidence(db:SqlDatabase,actor:WorkContext,input:{attachmentSetIds:string[];targetTypes:readonly string[];targetId:string;expectedTargetVersion:number;commandId:string;bindingType:string;bindingId:string;contexts:AttachmentBindingContract['contexts']}){
 if(!input.attachmentSetIds.length)return null;
 const ids=[...new Set(input.attachmentSetIds)],rows=(await db.prepare(`SELECT target_type FROM attachment_sets WHERE company_id=? AND id IN (${ids.map(()=>'?').join(',')})`).bind(actor.companyId,...ids).all<{target_type:string}>()).results,types=[...new Set(rows.map(row=>row.target_type))];
 if(rows.length!==ids.length||types.length!==1||!input.targetTypes.includes(types[0]))throw new ApiError(422,'ATTACHMENT_BINDING_CONTEXT','Choose files for this exact owner and action.');
 const contexts=input.contexts.filter(context=>attachmentPolicyRegistry.some(policy=>policy.targetType===types[0]&&policy.purposeKey===context.purposeKey&&policy.componentKinds.includes(context.componentKind)));
 if(!contexts.length)throw new ApiError(422,'ATTACHMENT_BINDING_CONTEXT','This owner has no registered file context for this action.');
 return prepareAttachmentBinding(db,actor,{attachmentSetIds:ids,bindingContract:{targetType:types[0],bindingType:input.bindingType,contexts},targetId:input.targetId,expectedTargetVersion:input.expectedTargetVersion,commandId:input.commandId,bindingId:input.bindingId});
}
