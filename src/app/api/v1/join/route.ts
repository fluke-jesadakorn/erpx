import { z } from 'zod';
import { sessionFromRequest } from '@/server/auth';
import { holdingBindingBySlug,holdingResources } from '@/server/cloudflare';
import { requireVerified } from '@/server/beta-onboarding';
import { acceptMember } from '@/server/beta-team';
import { betaBody } from '@/server/beta-http';
import { errorResponse } from '@/server/http';
export async function POST(request:Request){try{const input=z.object({holding:z.string().max(50),token:z.string().length(64)}).parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);const binding=await holdingBindingBySlug(input.holding),{write}=await holdingResources(binding.id);return Response.json({data:await acceptMember(write,user,input.token)});}catch(e){return errorResponse(e);}}
