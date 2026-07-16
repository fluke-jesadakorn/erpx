// Ambient declarations for Next.js modules that lib/ uses.
// lib/ is logically part of web-admin (consumed via @/* alias) but
// lives outside app/tsconfig.json's node_modules. This shim gives
// tsc the few symbols we touch without dragging in the full Next.js types.
//
// We only declare what lib/ actually references; app/* uses the real
// next/server types directly so this shim must NOT redeclare them.

declare module 'next/headers' {
  export function cookies(): Promise<{
    get(name: string): { value: string } | undefined;
    set(name: string, value: string, opts?: Record<string, unknown>): void;
    delete(name: string): void;
  }>;
  export function headers(): Promise<Headers>;
}

declare module 'next/server' {
  export interface NextRequest extends Request {
    nextUrl: URL & { clone(): URL & { pathname?: string; searchParams?: URLSearchParams } };
    cookies: { get(name: string): { value: string } | undefined };
    geo?: { city?: string; country?: string };
    ip?: string | null;
  }
  export class NextResponse<T = unknown> extends Response {
    constructor(body?: BodyInit | null, init?: ResponseInit);
    static json<T>(data: T, init?: ResponseInit | number): NextResponse<T>;
    static redirect(url: string | URL, status?: number): NextResponse;
    static next(): NextResponse;
  }
}