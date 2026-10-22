import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {AttachmentOperation} from '@erpx/server-files/public/attachments/types';
import {commercePackageEnforcementMode,readPurchasedAccesses,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
/** Neutral lot/serial evidence is shared stock property. Existing Supply stock
 * or an actual included stock foundation permits retained read history only;
 * the complete original Files IAM/Company policy must have authorized first. */
export async function authorizeInventoryLotAttachmentRead(database:SqlDatabase,actor:WorkContext,targetType:string,operation:AttachmentOperation){
 if(targetType!=='inventory_lot'||operation!=='read')return;
 const mode=await commercePackageEnforcementMode();if(mode==='disabled')return;
 const access=await readPurchasedAccesses(database,mode,actor.companyId,['supply.stock','inventory.stock-core']);
 const capability=access['supply.stock'].codes.history==='GRANTED'?'supply.stock':'inventory.stock-core';
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability,purpose:'history'});
}
