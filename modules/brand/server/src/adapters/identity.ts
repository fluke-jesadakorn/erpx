import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import type {ActiveBrandIdentity} from "@erpx/brand-server/domain/identity";
import {
 nativeBrandIdentityWorkspace,
 type NativeBrandIdentityWorkspace,
} from "@erpx/brand-ui/lib/native-brand-identity";

// All Brand identity SQL is owned by erpx-server-brand (Rust). These adapters
// are request-scoped projections of the mounted native identity workspace read.
type Active = NativeBrandIdentityWorkspace["active"];
let testActive:((actor:WorkContext)=>Promise<Active>) | undefined;
/** Test seam for callers that seed identity rows directly (integration oracles). */
export function setPublishedBrandIdentityForTest(active?: (actor: WorkContext) => Promise<Active>) { testActive = active; }

export async function publishedBrandIdentity(actor:WorkContext):Promise<ActiveBrandIdentity|null>;
export async function publishedBrandIdentity(db:SqlDatabase,companyId:string):Promise<ActiveBrandIdentity|null>;
export async function publishedBrandIdentity(actorOrDb:WorkContext|SqlDatabase,_companyId?:string):Promise<Active>{
 const actor="holdingSlug" in actorOrDb?actorOrDb:undefined;
 if(!actor)return null;
 if(testActive)return testActive(actor);
 const data=await nativeBrandIdentityWorkspace(actor,actor.holdingSlug);
 return (data?.active??null) as ActiveBrandIdentity|null;
}
export async function brandIdentityWorkspace(actor:WorkContext):Promise<NativeBrandIdentityWorkspace>{
 const data=await nativeBrandIdentityWorkspace(actor,actor.holdingSlug);
 if(!data)throw new ApiError(503,"NATIVE_BRAND_UNAVAILABLE","The native Brand identity reader is unavailable.");
 return data;
}
