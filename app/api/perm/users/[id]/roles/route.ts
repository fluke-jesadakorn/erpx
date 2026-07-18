import { NextResponse } from 'next/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PUT() {
  return NextResponse.json(
    { error: 'Use PUT /api/perm/users/:id/access for atomic access assignment' },
    { status: 410 },
  );
}
