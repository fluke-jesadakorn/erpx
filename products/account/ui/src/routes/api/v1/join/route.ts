import {canonicalDestinationLinks} from "@erpx/work-projection-ui/lib/destination-links";
import { z } from 'zod';
import { sessionFromRequest } from "@erpx/identity-access-server/public/account-session";
import { holdingBindingBySlug,holdingResources } from "@erpx/manage-erpx-server/public/cloudflare";
import { requireVerified } from "@erpx/manage-erpx-server/public/beta-onboarding";
import { acceptMember } from "@erpx/manage-erpx-server/public/beta-team";
import { betaBody } from "@erpx/manage-erpx-server/public/beta-http";
import { errorResponse } from "@erpx/platform-runtime-server/public/http";
export async function POST(request:Request){try{const input=z.object({holding:z.string().max(50),token:z.string().length(64)}).parse(await betaBody(request)),user=(await sessionFromRequest(request))?.user;requireVerified(user);const binding=await holdingBindingBySlug(input.holding),{write}=await holdingResources(binding.id);return Response.json(canonicalDestinationLinks({data:await acceptMember(write,user,input.token)}));}catch(e){return errorResponse(e);}}
