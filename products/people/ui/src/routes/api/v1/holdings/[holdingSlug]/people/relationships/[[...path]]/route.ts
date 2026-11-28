import { iamApi } from "@erpx/identity-access-server/public/iam-api";
export const dynamic = "force-dynamic";
async function handle(request:Request,{params}:{params:Promise<{holdingSlug:string;path?:string[]}>}) {
 const {holdingSlug,path=[]}=await params;
 return iamApi(request,holdingSlug,["people-relationships",...path]);
}
export {handle as GET,handle as POST};
