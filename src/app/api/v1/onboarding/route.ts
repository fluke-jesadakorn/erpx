import { sessionFromRequest } from '@/server/auth';
import { runtimeEnv } from '@/server/cloudflare';
import { onboard, onboardingSchema, requireVerified } from '@/server/beta-onboarding';
import { betaBody } from '@/server/beta-http';
import { errorResponse } from '@/server/http';
export async function GET(request:Request){try{const user=(await sessionFromRequest(request))?.user;requireVerified(user);const env=await runtimeEnv();const slot=await env.CONTROL_DB.prepare('SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?').bind(user.id).first();return Response.json({data:{enabled:env.ERPX_PUBLIC_BETA==='true',slot}});}catch(e){return errorResponse(e);}}
export async function POST(request:Request){try{const input=onboardingSchema.parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);return Response.json({data:await onboard(await runtimeEnv(),user,input)});}catch(e){return errorResponse(e);}}
