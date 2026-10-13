import { frontAppForHost } from "@/lib/shared/front-host";
import { storefrontAppIconPng } from "@/server/modules/work_projection/public/front-icon";
export async function GET(request:Request,{params}:{params:Promise<{size:string}>}) {
  const app=frontAppForHost(request.headers.get("host"));
  const {size:raw}=await params;
  if(!app||!["192","512"].includes(raw))return new Response("Not found",{status:404});
  const png=await storefrontAppIconPng({shortName:app.toUpperCase(),accentColor:app==="pms"?"#3659c9":"#137c67",app,size:Number(raw)});
  return new Response(new Uint8Array(png),{headers:{"Content-Type":"image/png","Cache-Control":"public, max-age=86400","X-Content-Type-Options":"nosniff"}});
}
