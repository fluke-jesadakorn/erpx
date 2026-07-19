import 'server-only';
import { createHash } from 'node:crypto';
import { Readable } from 'node:stream';
import ExcelJS from 'exceljs';
import { parse } from 'csv-parse';
import { query, withTransaction } from '@/db';

export type BankField = 'transactionDate' | 'valueDate' | 'description' | 'reference' | 'currency' | 'amount' | 'balance';
export type BankColumnMap = Partial<Record<BankField, string>>;

export interface NormalizedBankRow {
  rowNo: number;
  transactionDate: string;
  valueDate: string | null;
  description: string;
  reference: string | null;
  currency: string;
  amount: number;
  balance: number | null;
  fingerprint: string;
  raw: Record<string, unknown>;
}

export interface BankPreview {
  fileHash: string;
  headers: string[];
  mapping: BankColumnMap;
  rows: NormalizedBankRow[];
  duplicateFile: boolean;
  mappingComplete: boolean;
}

interface RawSheet {
  headers: string[];
  rows: Array<{ rowNo: number; values: Record<string, unknown>; formulas: Set<string> }>;
}

const aliases: Record<BankField, string[]> = {
  transactionDate: ['transaction date', 'date', 'วันที่', 'txn date', 'posting date'],
  valueDate: ['value date', 'effective date', 'วันที่มีผล'],
  description: ['description', 'details', 'รายการ', 'narrative', 'memo'],
  reference: ['reference', 'ref', 'เลขอ้างอิง', 'transaction id'],
  currency: ['currency', 'ccy', 'สกุลเงิน'],
  amount: ['amount', 'net amount', 'จำนวนเงิน', 'credit/debit', 'transaction amount'],
  balance: ['balance', 'running balance', 'ยอดคงเหลือ'],
};

const hash = (value: string | Buffer) => createHash('sha256').update(value).digest('hex');

function cleanHeader(value: unknown, index: number) {
  const text = String(value ?? '').trim();
  return text || `Column ${index + 1}`;
}

function autoMap(headers: string[]): BankColumnMap {
  const lower = new Map(headers.map((header) => [header.toLowerCase().trim(), header]));
  const mapping: BankColumnMap = {};
  for (const field of Object.keys(aliases) as BankField[]) {
    const match = aliases[field].find((name) => lower.has(name));
    if (match) mapping[field] = lower.get(match);
  }
  return mapping;
}

function valueOf(value: unknown) {
  if (value instanceof Date) return value;
  if (value && typeof value === 'object') {
    if ('result' in value) return (value as { result?: unknown }).result;
    if ('richText' in value) return (value as { richText: Array<{ text: string }> }).richText.map((item) => item.text).join('');
    if ('text' in value) return (value as { text: string }).text;
  }
  return value;
}

async function readXlsx(buffer: Buffer): Promise<RawSheet> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength) as ArrayBuffer);
  const sheet = workbook.worksheets[0];
  if (!sheet) throw new Error('The workbook has no worksheets');
  const headerRow = sheet.getRow(1);
  const headers: string[] = [];
  for (let column = 1; column <= headerRow.cellCount; column += 1) headers.push(cleanHeader(valueOf(headerRow.getCell(column).value), column - 1));
  const rows: RawSheet['rows'] = [];
  sheet.eachRow({ includeEmpty: false }, (row, rowNo) => {
    if (rowNo === 1) return;
    const values: Record<string, unknown> = {};
    const formulas = new Set<string>();
    headers.forEach((header, index) => {
      const cell = row.getCell(index + 1);
      if (cell.value && typeof cell.value === 'object' && 'formula' in cell.value) formulas.add(header);
      values[header] = valueOf(cell.value);
    });
    if (Object.values(values).some((value) => value !== null && value !== undefined && String(value).trim() !== '')) rows.push({ rowNo, values, formulas });
  });
  return { headers, rows };
}

async function readCsv(buffer: Buffer): Promise<RawSheet> {
  const records: Record<string, unknown>[] = [];
  const parser = Readable.from(buffer).pipe(parse({ columns: true, bom: true, skip_empty_lines: true, trim: true, relax_column_count: false }));
  for await (const record of parser) records.push(record as Record<string, unknown>);
  const headers = records[0] ? Object.keys(records[0]) : [];
  return { headers, rows: records.map((values, index) => ({ rowNo: index + 2, values, formulas: new Set<string>() })) };
}

function parseDate(value: unknown, field: string) {
  if (value instanceof Date && !Number.isNaN(value.getTime())) return value.toISOString().slice(0, 10);
  const text = String(value ?? '').trim();
  if (/^\d{4}-\d{2}-\d{2}$/.test(text)) return text;
  const dmy = text.match(/^(\d{1,2})[\/.\-](\d{1,2})[\/.\-](\d{4})$/);
  if (dmy) {
    const date = `${dmy[3]}-${dmy[2].padStart(2, '0')}-${dmy[1].padStart(2, '0')}`;
    const parsed = new Date(`${date}T00:00:00Z`);
    if (!Number.isNaN(parsed.getTime()) && parsed.toISOString().slice(0, 10) === date) return date;
  }
  throw new Error(`${field} is not a valid date: ${text || 'blank'}`);
}

function parseAmount(value: unknown, field: string) {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  let text = String(value ?? '').trim().replace(/\s/g, '');
  const negative = /^\(.*\)$/.test(text);
  text = text.replace(/[(),]/g, '');
  const parsed = Number(text);
  if (!Number.isFinite(parsed)) throw new Error(`${field} is not a valid number: ${String(value ?? '')}`);
  return negative ? -parsed : parsed;
}

function normalize(sheet: RawSheet, map: BankColumnMap, bankAccountId: number) {
  const required: BankField[] = ['transactionDate', 'description', 'amount'];
  for (const field of required) if (!map[field] || !sheet.headers.includes(map[field] as string)) throw new Error(`Map the required ${field} field`);
  return sheet.rows.map((row) => {
    for (const field of required) if (row.formulas.has(map[field] as string)) throw new Error(`Formula cells are not allowed in required fields (row ${row.rowNo}, ${field})`);
    const transactionDate = parseDate(row.values[map.transactionDate as string], `Row ${row.rowNo} transaction date`);
    const valueDate = map.valueDate && row.values[map.valueDate] ? parseDate(row.values[map.valueDate], `Row ${row.rowNo} value date`) : null;
    const description = String(row.values[map.description as string] ?? '').trim();
    if (!description) throw new Error(`Row ${row.rowNo} description is blank`);
    const amount = Math.round(parseAmount(row.values[map.amount as string], `Row ${row.rowNo} amount`) * 100) / 100;
    if (amount === 0) throw new Error(`Row ${row.rowNo} amount cannot be zero`);
    const balance = map.balance && row.values[map.balance] !== '' && row.values[map.balance] != null
      ? Math.round(parseAmount(row.values[map.balance], `Row ${row.rowNo} balance`) * 100) / 100
      : null;
    const reference = map.reference ? String(row.values[map.reference] ?? '').trim() || null : null;
    const currency = (map.currency ? String(row.values[map.currency] ?? '').trim() : 'THB').toUpperCase() || 'THB';
    if (!/^[A-Z]{3}$/.test(currency)) throw new Error(`Row ${row.rowNo} currency must be a three-letter code`);
    const fingerprint = hash([bankAccountId, transactionDate, valueDate, amount.toFixed(2), description.toLowerCase(), reference ?? ''].join('|'));
    return { rowNo: row.rowNo, transactionDate, valueDate, description, reference, currency, amount, balance, fingerprint, raw: row.values };
  });
}

export async function previewBankFile(args: { fileName: string; buffer: Buffer; bankAccountId: number; mapping?: BankColumnMap }) {
  const lower = args.fileName.toLowerCase();
  const sheet = lower.endsWith('.xlsx') ? await readXlsx(args.buffer) : lower.endsWith('.csv') ? await readCsv(args.buffer) : null;
  if (!sheet) throw new Error('Only CSV and XLSX bank statements are supported');
  const mapping = { ...autoMap(sheet.headers), ...(args.mapping ?? {}) };
  const mappingComplete = Boolean(mapping.transactionDate && mapping.description && mapping.amount);
  const rows = mappingComplete ? normalize(sheet, mapping, args.bankAccountId) : [];
  const fingerprints = new Set<string>();
  for (const row of rows) {
    if (fingerprints.has(row.fingerprint)) throw new Error(`Duplicate statement row detected at row ${row.rowNo}`);
    fingerprints.add(row.fingerprint);
  }
  const fileHash = hash(args.buffer);
  const duplicate = await query<{ present: boolean }>(`SELECT EXISTS (SELECT 1 FROM finance.bank_imports WHERE file_hash = $1) AS present`, [fileHash]);
  return { fileHash, headers: sheet.headers, mapping, rows, duplicateFile: Boolean(duplicate.rows[0]?.present), mappingComplete } satisfies BankPreview;
}

export async function commitBankFile(args: {
  fileName: string;
  buffer: Buffer;
  bankAccountId: number;
  mapping: BankColumnMap;
  templateName?: string;
  actorId: number;
}) {
  const preview = await previewBankFile(args);
  if (preview.duplicateFile) throw new Error('This bank statement file was already imported');
  if (!preview.mappingComplete || !preview.rows.length) throw new Error('Complete the required column mapping and preview at least one row');
  return withTransaction(async (q) => {
    const account = await q<{ currency_code: string }>(`SELECT currency_code FROM finance.bank_accounts WHERE id = $1 AND active FOR SHARE`, [args.bankAccountId]);
    if (!account.rows[0]) throw new Error('Bank account not found');
    const currencySet = new Set(preview.rows.map((row) => row.currency));
    if (currencySet.size > 1 || !currencySet.has(account.rows[0].currency_code.trim())) throw new Error('Statement currency does not match the bank account');
    const fingerprints = preview.rows.map((row) => row.fingerprint);
    const prior = await q<{ row_fingerprint: string }>(`SELECT row_fingerprint FROM finance.bank_transactions WHERE bank_account_id = $1 AND row_fingerprint = ANY($2::text[])`, [args.bankAccountId, fingerprints]);
    if (prior.rows.length) throw new Error(`${prior.rows.length} statement row(s) were already imported`);
    let templateId: number | null = null;
    if (args.templateName?.trim()) {
      const template = await q<{ id: string }>(
        `INSERT INTO finance.bank_import_templates(bank_account_id, name, mapping, created_by)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (bank_account_id, name) DO UPDATE SET mapping = excluded.mapping
         RETURNING id::text`,
        [args.bankAccountId, args.templateName.trim(), args.mapping, args.actorId],
      );
      templateId = Number(template.rows[0].id);
    }
    const imported = await q<{ id: string }>(
      `INSERT INTO finance.bank_imports(bank_account_id, file_name, file_hash, template_id, row_count, imported_by)
       VALUES ($1,$2,$3,$4,$5,$6) RETURNING id::text`,
      [args.bankAccountId, args.fileName, preview.fileHash, templateId, preview.rows.length, args.actorId],
    );
    const importId = Number(imported.rows[0].id);
    for (const row of preview.rows) {
      await q(
        `INSERT INTO finance.bank_transactions
           (import_id, bank_account_id, row_no, row_fingerprint, transaction_date,
            value_date, description, reference, currency_code, amount, balance, raw)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,
        [importId, args.bankAccountId, row.rowNo, row.fingerprint, row.transactionDate, row.valueDate, row.description, row.reference, row.currency, row.amount, row.balance, row.raw],
      );
    }
    return { importId, rowCount: preview.rows.length, fileHash: preview.fileHash };
  });
}
