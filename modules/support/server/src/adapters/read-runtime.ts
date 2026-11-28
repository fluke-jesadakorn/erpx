import 'server-only';
import type{WorkContext}from '@erpx/identity-access-server/public/authorization';
import type{SqlDatabase}from '@erpx/platform-runtime-server/public/runtime/types';
import{holdingResources}from '@erpx/platform-runtime-server/public/runtime';
import{ApiError}from '@erpx/platform-runtime-server/public/http';
import{requirePermission}from '@erpx/identity-access-server/public/iam-permissions';
import{readPurchasedAccess,supportPackageEnforcementMode,type PurchasedAccess}from '@erpx/commercial-server/public/purchase-command-boundary';
import type{CapabilityPurpose}from '@erpx/commercial-server/public/packages';
export function nativeSupportReadRuntime(actor:WorkContext,supplied?:SqlDatabase){let db = supplied;
const database = async () => db ??= (await holdingResources(actor.holdingId)).write;
const access = new Map<string, Promise<PurchasedAccess>>();
const purchase = (capability: string) => {
    let pending = access.get(capability);
    if (!pending) { pending = (async () => readPurchasedAccess(database, await supportPackageEnforcementMode(), { companyId: actor.companyId, capability }))(); access.set(capability, pending); }
    return pending;
  };
const requirePurchase = async (capability: string, purpose: CapabilityPurpose = "history") => {
    const value = await purchase(capability), allowed = purpose === "new-work" ? value.newWork : purpose === "settle-existing" ? value.settleExisting : value.history;
    if (!allowed) throw new ApiError(403, value.codes[purpose], "This package does not permit the requested work.");
    return value;
  };return{database,purchase,requirePurchase,authorize:requirePermission,error:(status:number,code:string,message:string)=>new ApiError(status,code,message),now:Date.now};}
