import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {AttachmentOperation} from '../../files/public/attachments/types';
import {commercePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from '../../commercial/public/purchase-command-boundary';
/** Neutral product evidence uses the existing shared Stock foundation. Existing Supply stock
 * or an actual included stock foundation permits retained read history only;
 * the complete original Files IAM/Company/Product or POS fallback policy must have authorized first. */
export async function authorizeCommerceProductAttachmentRead(database:SqlDatabase,actor:WorkContext,targetType:string,operation:AttachmentOperation){
 if(targetType!=='product'||operation!=='read')return;
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;
 const access=await readPurchasedAccesses(database,mode,actor.companyId,['supply.stock','inventory.stock-core']);
 const capability=access['supply.stock'].codes.history==='GRANTED'?'supply.stock':'inventory.stock-core';
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:'history'});
}
