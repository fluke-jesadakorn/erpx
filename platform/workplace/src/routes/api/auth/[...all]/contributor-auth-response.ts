/** Projects the transitional Node Account response onto the native Account DTO.
 * Called only by the explicitly selected contributor Node route. */
type Kind = 'sign-in' | 'session';
export function shouldProjectContributorAuth(profile: boolean, host: string | null, origin: string): boolean {
  return profile && host === 'account.erpx.localhost:3443' && origin === 'https://account.erpx.localhost:3443';
}
const object = (x: unknown): x is Record<string, unknown> => typeof x === 'object' && x !== null && !Array.isArray(x);
const text = (x: unknown): x is string => typeof x === 'string' && x.length > 0 && x.length <= 4096;
const date = (x: unknown): x is string => typeof x === 'string' && /^\d{4}-\d{2}-\d{2}T/.test(x) && Number.isFinite(Date.parse(x));
function user(value: unknown) {
  if (!object(value) || !text(value.id) || !text(value.name) || !text(value.email) || typeof value.emailVerified !== 'boolean' || !(value.image === null || text(value.image))) return null;
  return { id: value.id, name: value.name, email: value.email, emailVerified: value.emailVerified, image: value.image };
}
function project(value: unknown, kind: Kind): unknown {
  if (kind === 'session' && value === null) return null;
  if (!object(value)) throw Error('Invalid transitional Account response');
  const projectedUser = user(value.user);
  if (!projectedUser) throw Error('Invalid transitional Account user');
  if (kind === 'sign-in') return { user: projectedUser };
  const s = value.session;
  if (!object(s) || !text(s.id) || s.userId !== projectedUser.id || !date(s.expiresAt) || !date(s.createdAt) || !date(s.updatedAt)) throw Error('Invalid transitional Account session');
  return { user: projectedUser, session: { id: s.id, userId: s.userId, expiresAt: s.expiresAt, createdAt: s.createdAt, updatedAt: s.updatedAt } };
}
export async function contributorAuthResponse(response: Response, kind: Kind): Promise<Response> {
  if (!response.ok || response.status === 204 || response.status === 205) return response;
  try {
    const body = JSON.stringify(project(await response.json(), kind));
    const headers = new Headers(response.headers);
    headers.delete('content-length'); headers.delete('content-encoding');
    headers.set('content-type', 'application/json');
    return new Response(body, { status: response.status, statusText: response.statusText, headers });
  } catch {
    return Response.json({ error: { code: 'ACCOUNT_RESPONSE_INVALID', message: 'The Account response did not match its contract.' } }, { status: 502, headers: { 'Cache-Control': 'private, no-store' } });
  }
}
