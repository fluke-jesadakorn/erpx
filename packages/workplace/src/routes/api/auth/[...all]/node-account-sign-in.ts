export function isLegacyMultiSessionPath(path: string): boolean { return path === "/api/auth/multi-session" || path.startsWith("/api/auth/multi-session/"); }

/** A browser receives the authenticated cookie, never Better Auth's session token. */
export async function nodeAccountSignInResponse(response: Response): Promise<Response> {
  if (!response.ok) return response;
  try {
    const value: unknown = await response.json();
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw Error('INVALID_ACCOUNT_RESPONSE');
    const body = value as Record<string, unknown>;
    if (!body.user || typeof body.user !== 'object' || Array.isArray(body.user) || typeof body.token !== 'string' || !body.token) throw Error('INVALID_ACCOUNT_RESPONSE');
    const user = body.user as Record<string, unknown>;
    if (typeof user.id !== 'string' || !user.id || typeof user.name !== 'string' || !user.name || typeof user.email !== 'string' || !user.email || typeof user.emailVerified !== 'boolean' || !(user.image === null || typeof user.image === 'string') || typeof body.redirect !== 'boolean' || !(body.url === null || typeof body.url === 'string')) throw Error('INVALID_ACCOUNT_RESPONSE');
    const publicUser = Object.fromEntries(['id','name','email','emailVerified','image','createdAt','updatedAt'].filter(key => key in user).map(key => [key,user[key]]));
    const publicBody = {redirect:body.redirect,url:body.url,user:publicUser};
    const headers = new Headers(response.headers);
    headers.delete('content-length'); headers.delete('content-encoding'); headers.set('content-type','application/json');
    return new Response(JSON.stringify(publicBody), {status:response.status,statusText:response.statusText,headers});
  } catch {
    return Response.json({error:{code:'ACCOUNT_RESPONSE_INVALID',message:'Account response did not match its contract.'}}, {status:502,headers:{'Cache-Control':'private, no-store'}});
  }
}

/** Better Auth may return a new session token during invitation sign-up. */
export async function nodeAccountSignUpResponse(response: Response): Promise<Response> {
  if (!response.ok) return response;
  try {
    const value: unknown = await response.json();
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw Error('INVALID_ACCOUNT_RESPONSE');
    const body = value as Record<string, unknown>;
    if (!body.user || typeof body.user !== 'object' || Array.isArray(body.user) || !(body.token === null || typeof body.token === 'string')) throw Error('INVALID_ACCOUNT_RESPONSE');
    const user = body.user as Record<string, unknown>;
    if (typeof user.id !== 'string' || !user.id || typeof user.name !== 'string' || !user.name || typeof user.email !== 'string' || !user.email || typeof user.emailVerified !== 'boolean' || !(user.image === null || typeof user.image === 'string')) throw Error('INVALID_ACCOUNT_RESPONSE');
    const publicUser = Object.fromEntries(['id','name','email','emailVerified','image','createdAt','updatedAt'].filter(key => key in user).map(key => [key,user[key]]));
    const headers = new Headers(response.headers); headers.delete('content-length'); headers.delete('content-encoding'); headers.set('content-type','application/json');
    return new Response(JSON.stringify({user:publicUser}), {status:response.status,statusText:response.statusText,headers});
  } catch {
    return Response.json({error:{code:'ACCOUNT_RESPONSE_INVALID',message:'Account response did not match its contract.'}}, {status:502,headers:{'Cache-Control':'private, no-store'}});
  }
}
