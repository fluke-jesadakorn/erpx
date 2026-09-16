import { iamAccountApi } from "@/server/iam-api";
export const dynamic="force-dynamic";
async function handle(request:Request,{params}:{params:Promise<{path?:string[]}>}){return iamAccountApi(request,(await params).path??[]);}
export {handle as GET,handle as POST};
