import { NextResponse } from 'next/server';
import { config } from '@folio-lib/config';
import { query } from '@folio-lib/db';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  let dbOk = false;
  try {
    await query('SELECT 1');
    dbOk = true;
  } catch {}
  return NextResponse.json({
    ok: true, service: 'slips-route', db: dbOk,
    bucket: config.storage.minio.bucket, ts: new Date().toISOString(),
    note: 'POST /api/slips is deprecated; use POST /api/upload',
  });
}