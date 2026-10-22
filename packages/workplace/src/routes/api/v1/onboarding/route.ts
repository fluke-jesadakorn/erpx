import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { sessionFromRequest } from "@erpx/server-identity_access/public/auth";
import { runtimeEnv } from "@erpx/server-manage_erpx/public/cloudflare";
import { onboard, onboardingSchema, requireVerified, onboardingView } from "@erpx/server-manage_erpx/public/beta-onboarding";
import { betaBody } from "@erpx/server-manage_erpx/public/beta-http";
import { errorResponse } from "@erpx/server-runtime/public/http";
export async function GET(request:Request){try{const user=(await sessionFromRequest(request))?.user;requireVerified(user);const env=await runtimeEnv();const data=await onboardingView(env,user);return Response.json(canonicalDestinationLinks({data}));}catch(e){return errorResponse(e);}}
export async function POST(request:Request){try{const input=onboardingSchema.parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);return Response.json(canonicalDestinationLinks({data:await onboard(await runtimeEnv(),user,input)}));}catch(e){return errorResponse(e);}}
