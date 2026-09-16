import { errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { createAuth } from "@/server/auth";

export const dynamic = "force-dynamic";

export async function GET(request: Request) {
  const callback = new URL(request.url).pathname.match(/\/callback\/directory-([a-f0-9-]{36})$/i);
  try{return await (await createAuth(requestPublicOrigin(request),callback?.[1])).handler(request);}catch(e){return errorResponse(e);}
}

export async function POST(request: Request) {
  let directoryId: string | undefined;
  if(new URL(request.url).pathname.endsWith("/sign-in/social")){const body=await request.clone().json().catch(()=>null);const provider=body?.provider; if(typeof provider==="string"&&/^directory-[a-f0-9-]{36}$/i.test(provider))directoryId=provider.slice(10);}
  try{return await (await createAuth(requestPublicOrigin(request),directoryId)).handler(request);}catch(e){return errorResponse(e);}
}
