import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { previewBankFile, type BankColumnMap } from '@/finance/bankImport';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'finance:bank:import::allow' });
  if (guard.response) return guard.response;
  try {
    const form = await req.formData();
    const file = form.get('file');
    if (!(file instanceof File) || file.size === 0) return NextResponse.json({ error: 'A CSV or XLSX file is required' }, { status: 400 });
    const bankAccountId = Number(form.get('bank_account_id'));
    const mapping = JSON.parse(String(form.get('mapping') || '{}')) as BankColumnMap;
    const preview = await previewBankFile({ fileName: file.name, buffer: Buffer.from(await file.arrayBuffer()), bankAccountId, mapping });
    return NextResponse.json(preview);
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : 'Unable to preview bank file' }, { status: 400 });
  }
}
