import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from 'zod';
import { sessionFromRequest } from "@erpx/server-identity_access/public/auth";
import { holdingBindingBySlug,holdingResources } from "@erpx/server-manage_erpx/public/cloudflare";
import { requireVerified } from "@erpx/server-manage_erpx/public/beta-onboarding";
import { acceptMember } from "@erpx/server-manage_erpx/public/beta-team";
import { betaBody } from "@erpx/server-manage_erpx/public/beta-http";
import { errorResponse } from "@erpx/server-runtime/public/http";
export async function POST(request:Request){try{const input=z.object({holding:z.string().max(50),token:z.string().length(64)}).parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);const binding=await holdingBindingBySlug(input.holding),{write}=await holdingResources(binding.id);return Response.json(canonicalDestinationLinks({data:await acceptMember(write,user,input.token)}));}catch(e){return errorResponse(e);}}
