import type { APIRequestContext } from '@playwright/test';

export interface UploadSlipOptions {
  kind?: 'receipt' | 'book_bank';
  fileName?: string;
  content?: Buffer | string;
  mimeType?: string;
  modelName?: string;
}

export interface UploadSlipResult {
  slipId: number;
  status: string;
  fileKey: string;
  parsed: Record<string, unknown>;
  confidence: number | null;
  validation: { ok: boolean; errors: unknown[]; warnings: unknown[] };
}

function defaultReceipt(): Buffer {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="600" height="800"><rect width="600" height="800" fill="#fff"/><text x="40" y="60" font-size="32" font-family="sans-serif">RECEIPT</text><text x="40" y="120" font-size="20" font-family="sans-serif">Vendor: Test Vendor Co</text><text x="40" y="160" font-size="20" font-family="sans-serif">Date: 2026-07-18</text><text x="40" y="220" font-size="20" font-family="sans-serif">Subtotal: 100.00</text><text x="40" y="260" font-size="20" font-family="sans-serif">VAT 7%: 7.00</text><text x="40" y="320" font-size="28" font-family="sans-serif">Total: 107.00</text></svg>`;
  return Buffer.from(svg, 'utf-8');
}

function defaultBank(): Buffer {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="600" height="400"><rect width="600" height="400" fill="#fff"/><text x="40" y="60" font-size="28" font-family="sans-serif">BANK BOOK</text><text x="40" y="120" font-size="20" font-family="sans-serif">Bank: Bangkok Bank</text><text x="40" y="160" font-size="20" font-family="sans-serif">Branch: Silom</text><text x="40" y="200" font-size="20" font-family="sans-serif">Account: 123-4-56789-0</text><text x="40" y="240" font-size="20" font-family="sans-serif">Name: John Doe</text></svg>`;
  return Buffer.from(svg, 'utf-8');
}

export async function uploadSlip(req: APIRequestContext, opts: UploadSlipOptions = {}): Promise<UploadSlipResult> {
  const kind = opts.kind ?? 'receipt';
  const fileName = opts.fileName ?? (kind === 'book_bank' ? 'bank.svg' : 'receipt.svg');
  const content = opts.content ?? (kind === 'book_bank' ? defaultBank() : defaultReceipt());
  const mimeType = opts.mimeType ?? 'image/svg+xml';
  return await (req as unknown as { post: (u: string, o: unknown) => Promise<UploadSlipResult> }).post('/api/upload', {
    multipart: {
      file: { name: fileName, mimeType, buffer: content },
      kind,
      ...(opts.modelName ? { model_name: opts.modelName } : {}),
    },
  });
}
