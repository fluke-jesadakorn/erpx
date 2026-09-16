import { iamApi } from "@/server/iam-api";
export const dynamic="force-dynamic";
async function handle(request:Request,{params}:{params:Promise<{holdingSlug:string;path?:string[]}>}){const p=await params;return iamApi(request,p.holdingSlug,p.path??[]);}
export {handle as GET,handle as POST};
