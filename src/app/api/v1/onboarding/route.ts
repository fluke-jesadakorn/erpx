import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { sessionFromRequest } from "@/server/modules/identity_access/public/auth";
import { runtimeEnv } from "@/server/modules/manage_erpx/public/cloudflare";
import { onboard, onboardingSchema, requireVerified, onboardingView } from "@/server/modules/manage_erpx/public/beta-onboarding";
import { betaBody } from "@/server/modules/manage_erpx/public/beta-http";
import { errorResponse } from "@/server/modules/runtime/public/http";
export async function GET(request:Request){try{const user=(await sessionFromRequest(request))?.user;requireVerified(user);const env=await runtimeEnv();const data=await onboardingView(env,user);return Response.json(canonicalDestinationLinks({data}));}catch(e){return errorResponse(e);}}
export async function POST(request:Request){try{const input=onboardingSchema.parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);return Response.json(canonicalDestinationLinks({data:await onboard(await runtimeEnv(),user,input)}));}catch(e){return errorResponse(e);}}
