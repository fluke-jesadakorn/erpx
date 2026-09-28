import { errorResponse } from "@/server/http";
import { requestPublicOrigin } from "@/server/public-origin";
import { createAuth } from "@/server/auth";
import { frontAppForHost, safeFrontReturnPath } from "@/lib/front-host";

export const dynamic = "force-dynamic";

async function frontCallbackAllowed(request: Request) {
  if (!frontAppForHost(request.headers.get("host"))) return true;
  const url = new URL(request.url);
  const values = [url.searchParams.get("callbackURL"), url.searchParams.get("redirectTo")];
  if (request.method === "POST") {
    const body = await request.clone().json().catch(() => null);
    if (body && typeof body === "object") for (const key of ["callbackURL", "redirectTo", "newUserCallbackURL", "errorCallbackURL"])
      if (typeof body[key] === "string") values.push(body[key]);
  }
  return values.every((value) => value === null || (value.startsWith("/") && safeFrontReturnPath(value) === value));
}

export async function GET(request: Request) {
  if (!await frontCallbackAllowed(request)) return new Response("Invalid return path", { status: 400 });
  const callback = new URL(request.url).pathname.match(/\/callback\/directory-([a-f0-9-]{36})$/i);
  try{return await (await createAuth(requestPublicOrigin(request),callback?.[1])).handler(request);}catch(e){return errorResponse(e);}
}

export async function POST(request: Request) {
  if (!await frontCallbackAllowed(request)) return new Response("Invalid return path", { status: 400 });
  let directoryId: string | undefined;
  if(new URL(request.url).pathname.endsWith("/sign-in/social")){const body=await request.clone().json().catch(()=>null);const provider=body?.provider; if(typeof provider==="string"&&/^directory-[a-f0-9-]{36}$/i.test(provider))directoryId=provider.slice(10);}
  try{return await (await createAuth(requestPublicOrigin(request),directoryId)).handler(request);}catch(e){return errorResponse(e);}
}
