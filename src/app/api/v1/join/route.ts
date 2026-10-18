import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { z } from 'zod';
import { sessionFromRequest } from "@/server/modules/identity_access/public/auth";
import { holdingBindingBySlug,holdingResources } from "@/server/modules/manage_erpx/public/cloudflare";
import { requireVerified } from "@/server/modules/manage_erpx/public/beta-onboarding";
import { acceptMember } from "@/server/modules/manage_erpx/public/beta-team";
import { betaBody } from "@/server/modules/manage_erpx/public/beta-http";
import { errorResponse } from "@/server/modules/runtime/public/http";
export async function POST(request:Request){try{const input=z.object({holding:z.string().max(50),token:z.string().length(64)}).parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);const binding=await holdingBindingBySlug(input.holding),{write}=await holdingResources(binding.id);return Response.json(canonicalDestinationLinks({data:await acceptMember(write,user,input.token)}));}catch(e){return errorResponse(e);}}
