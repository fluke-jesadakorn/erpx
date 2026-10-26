import { errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { createAuth } from "@erpx/server-identity_access/public/auth";
import { frontAppForHost, safeFrontReturnPath } from "@erpx/feature-shared/lib/front-host";
import { productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { productSignOut } from "@erpx/server-identity_access/public/product-sso";
import { anonymousAccountAuthPath } from "@erpx/server-identity_access/public/product-sso";
import { accountEmailSignIn, accountAuthenticationRequest, synchronizeAccountAuth } from '@erpx/server-identity_access/public/device-accounts';

export const dynamic = "force-dynamic";

async function accountSessionDenied(request: Request) {
  if (productHostForHost(request.headers.get("host")) !== "account" || anonymousAccountAuthPath(new URL(request.url).pathname)) return null;
  const session = await (await createAuth(requestPublicOrigin(request))).api.getSession({ headers: request.headers });
  return session ? null : Response.json({ error: { code: "UNAUTHENTICATED", message: "Sign in to ERPX Account." } }, { status: 401, headers: { "Cache-Control": "private, no-store" } });
}

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
  const audience = productHostForHost(request.headers.get("host"));
  if (audience && new URL(request.url).pathname === "/api/auth/get-session") {
    const session = await (await createAuth(requestPublicOrigin(request))).api.getSession({ headers: request.headers });
    return Response.json(session ? { ...session, session: { ...session.session, token: undefined } } : null, { headers: { "Cache-Control": "private, no-store" } });
  }
  if (audience && audience !== "account") return new Response("Not found", { status: 404 });
  const denied = await accountSessionDenied(request);
  if (denied) return denied;
  if (!await frontCallbackAllowed(request)) return new Response("Invalid return path", { status: 400 });
  const callback = new URL(request.url).pathname.match(/\/callback\/directory-([a-f0-9-]{36})$/i);
  try{const response=await (await createAuth(requestPublicOrigin(request),callback?.[1])).handler(audience==='account'?await accountAuthenticationRequest(request):request);return audience==='account'?await synchronizeAccountAuth(request,response):response;}catch(e){return errorResponse(e);}
}

export async function POST(request: Request) {
  const audience = productHostForHost(request.headers.get("host"));
  if (audience && new URL(request.url).pathname === "/api/auth/sign-out") return (await productSignOut(request))!;
  if (audience && audience !== "account") return new Response("Not found", { status: 404 });
  if (audience === 'account' && new URL(request.url).pathname === '/api/auth/sign-in/email') {
    if(request.headers.get('origin')!==requestPublicOrigin(request)) return new Response('Invalid origin',{status:403});
    try{return await accountEmailSignIn(request);}catch(error){return errorResponse(error);}
  }
  const denied = await accountSessionDenied(request);
  if (denied) return denied;
  if (!await frontCallbackAllowed(request)) return new Response("Invalid return path", { status: 400 });
  let directoryId: string | undefined;
  if(new URL(request.url).pathname.endsWith("/sign-in/social")){const body=await request.clone().json().catch(()=>null);const provider=body?.provider; if(typeof provider==="string"&&/^directory-[a-f0-9-]{36}$/i.test(provider))directoryId=provider.slice(10);}
  try{const response=await (await createAuth(requestPublicOrigin(request),directoryId)).handler(audience==='account'?await accountAuthenticationRequest(request):request);return audience==='account'?await synchronizeAccountAuth(request,response):response;}catch(e){return errorResponse(e);}
}
