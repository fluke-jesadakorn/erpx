// Middleware: verify HMAC-signed session on protected routes.
// - /api/* routes: 401 if cookie/header invalid (except public paths like /api/actor, /api/ai/invoke).
// - Pages: redirect to /?login=1

import { NextRequest, NextResponse } from 'next/server';
import { SESSION_COOKIE, verifySession } from '@folio-lib/server/sessionToken';

const PUBLIC_PATHS = [
  '/login',
];

const PUBLIC_API = [
  '/api/actor',
  '/api/health',
  '/api/ai/invoke',  // Accepts x-ai-key bridge header; route itself checks auth.
  '/api/hook/line',     // webhook: signature verified by provider secret
  '/api/hook/generic',  // webhook: signature verified by provider secret
];

export async function middleware(req: NextRequest) {
  const { pathname } = req.nextUrl;

  const isPublicPath =
    pathname === '/' ||
    PUBLIC_PATHS.some((p) => p !== '/' && (pathname === p || pathname.startsWith(`${p}/`)));
  const isPublicApi =
    pathname === '/api/actor' ||
    pathname.startsWith('/api/actor/') ||
    pathname === '/api/auth/sign-in' ||
    pathname.startsWith('/api/auth/sign-in/') ||
    pathname === '/api/me/locale' ||
    PUBLIC_API.some((p) => pathname === p);

  const token = req.cookies.get(SESSION_COOKIE)?.value ?? req.headers.get('x-folio-session');
  const payload = await verifySession(token);

  if (payload || isPublicPath || isPublicApi) {
    return NextResponse.next();
  }

  if (pathname.startsWith('/api/')) {
    return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  }

  const url = req.nextUrl.clone();
  url.pathname = '/';
  url.searchParams.set('login', '1');
  url.searchParams.set('next', pathname);
  return NextResponse.redirect(url);
}

export const config = {
  matcher: [
    // Run on everything except static assets and the Next.js internals.
    '/((?!_next/static|_next/image|favicon.ico|.*\\.svg).*)',
  ],
};