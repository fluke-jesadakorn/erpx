import { NextResponse } from 'next/server';
import { get } from '@/slips/storage';
import { apiSlipGuard } from '@/server/apiGuard';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const url = new URL(req.url);
  const key = url.searchParams.get('key');
  if (!key || typeof key !== 'string') return NextResponse.json({ error: 'key required' }, { status: 400 });
  if (key.includes('..')) return NextResponse.json({ error: 'invalid key' }, { status: 400 });

  const guard = await apiSlipGuard(req, key);
  if (guard.response) return guard.response;

  try {
    const buffer = await get(key);

    let contentType = 'application/octet-stream';
    const lowerKey = key.toLowerCase();
    if (lowerKey.endsWith('.png')) contentType = 'image/png';
    else if (lowerKey.endsWith('.jpg') || lowerKey.endsWith('.jpeg')) contentType = 'image/jpeg';
    else if (lowerKey.endsWith('.webp')) contentType = 'image/webp';
    else if (lowerKey.endsWith('.pdf')) contentType = 'application/pdf';

    return new Response(buffer as any, {
      status: 200,
      headers: {
        'Content-Type': contentType,
        'Cache-Control': 'public, max-age=31536000, immutable',
      },
    });
  } catch (err) {
    console.error('Error serving slip file:', err);
    return NextResponse.json({ error: 'not found' }, { status: 404 });
  }
}