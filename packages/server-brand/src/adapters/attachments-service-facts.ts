import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
// Wave 3: brand publication effects execute only in the native axum branding
// route (branding.rs). No TypeScript SQL writer remains for this capability;
// the effect methods stay present with their retained signatures and fail
// closed so the Files producer adopts the native publication port.
function nativeBrandWriteUnavailable():never{
  throw new ApiError(503,'NATIVE_BRANDING_UNAVAILABLE','The native Brand publication command is unavailable.');
}
export function attachmentServiceBrandFacts(_db: SqlDatabase) {
    return {
      stepOrganizationBrandProfiles71: (_facts: unknown):never => nativeBrandWriteUnavailable(),
      stepStorefrontAppBrands72: (_facts: unknown):never => nativeBrandWriteUnavailable(),
      stepOrganizationBrandProfiles73: (_facts: unknown):never => nativeBrandWriteUnavailable(),
      stepStorefrontAppBrands74: (_facts: unknown):never => nativeBrandWriteUnavailable(),
    };
}
