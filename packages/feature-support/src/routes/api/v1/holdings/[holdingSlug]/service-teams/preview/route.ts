import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {createNativeSupportServiceTeams} from '../../../../../../../adapters/server/native-support-service-teams';
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
export async function POST(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{
 const {holdingSlug}=await context.params;await requireWorkContext(holdingSlug);
 const config=process.env.ERPX_NATIVE_SUPPORT_SERVICE_TEAMS_CONFIG;
 if(!config)throw new ApiError(503,'NATIVE_SUPPORT_SERVICE_TEAMS_UNAVAILABLE','The native Support service-team service is unavailable.');
 return Response.json(canonicalDestinationLinks({data:await createNativeSupportServiceTeams(config).preview(request.headers,holdingSlug,await request.text())}),{headers:{"Cache-Control":"private, no-store"}});
}catch(error){return errorResponse(error);}}
