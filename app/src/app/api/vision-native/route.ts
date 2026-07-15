import { NextResponse } from 'next/server';
import { createRequire } from 'node:module';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  try {
    const require = createRequire(`${process.cwd()}/`);
    const mod = require('../lib/native/vision-ocr/index.js');
    return NextResponse.json({ ocrAvailable: typeof mod.ocrAvailable === 'function' ? mod.ocrAvailable() : false });
  } catch {
    return NextResponse.json({ ocrAvailable: false });
  }
}