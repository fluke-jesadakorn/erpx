import { iamApi } from "@/server/modules/identity_access/public/iam-api";
export const dynamic="force-dynamic";
async function handle(request:Request,{params}:{params:Promise<{path?:string[]}>}){return iamApi(request,null,(await params).path??[]);}
export {handle as GET,handle as POST};
