import 'server-only';

import { randomUUID } from 'node:crypto';
import path from 'node:path';
import { query } from '@/db';
import { put, remove } from '@/slips/storage';

const bucket = process.env.MINIO_BUCKET ?? 'folio-storage';
const prefix = 'law/contracts/';

interface ContractRow {
  id: string;
  line_user_id: string | null;
  line_group_id: string | null;
  line_message_id: string | null;
  file_name: string;
  file_type: string | null;
  file_mime: string | null;
  storage_bucket: string | null;
  storage_path: string | null;
  size_bytes: string | number | null;
  chunk_count: number;
  status: string;
  error_message: string | null;
  uploaded_at: Date | string;
  doc_no: string | null;
  category: string | null;
  source: string | null;
  metadata: unknown;
  updated_at: Date | string | null;
}

export interface Contract {
  id: string;
  lineUserId: string | null;
  lineGroupId: string | null;
  lineMessageId: string | null;
  fileName: string;
  fileType: string | null;
  fileMime: string | null;
  storageBucket: string | null;
  storagePath: string | null;
  sizeBytes: number | null;
  chunkCount: number;
  status: string;
  errorMessage: string | null;
  uploadedAt: string;
  docNo: string | null;
  category: string | null;
  source: string | null;
  metadata: unknown;
  updatedAt: string | null;
}

const cols = `id, line_user_id, line_group_id, line_message_id, file_name,
  file_type, file_mime, storage_bucket, storage_path, size_bytes, chunk_count,
  status, error_message, uploaded_at, doc_no, category, source, metadata, updated_at`;

function iso(v: Date | string | null): string | null {
  if (v == null) return null;
  return new Date(v).toISOString();
}

function map(row: ContractRow): Contract {
  return {
    id: row.id,
    lineUserId: row.line_user_id,
    lineGroupId: row.line_group_id,
    lineMessageId: row.line_message_id,
    fileName: row.file_name,
    fileType: row.file_type,
    fileMime: row.file_mime,
    storageBucket: row.storage_bucket,
    storagePath: row.storage_path,
    sizeBytes: row.size_bytes == null ? null : Number(row.size_bytes),
    chunkCount: Number(row.chunk_count),
    status: row.status,
    errorMessage: row.error_message,
    uploadedAt: iso(row.uploaded_at) as string,
    docNo: row.doc_no,
    category: row.category,
    source: row.source,
    metadata: row.metadata,
    updatedAt: iso(row.updated_at),
  };
}

export async function listContracts(
  opts: { status?: string; limit?: number } = {},
): Promise<Contract[]> {
  const status = opts.status?.trim() || null;
  const limit = Math.min(Math.max(Math.floor(opts.limit ?? 50), 1), 200);
  const r = await query<ContractRow>(
    `SELECT ${cols}
       FROM law.contracts
      WHERE ($1::text IS NULL OR status = $1)
      ORDER BY uploaded_at DESC
      LIMIT $2`,
    [status, limit],
  );
  return r.rows.map(map);
}

export async function getContract(id: string): Promise<Contract | null> {
  const r = await query<ContractRow>(
    `SELECT ${cols}
       FROM law.contracts
      WHERE id = $1::uuid`,
    [id],
  );
  return r.rows[0] ? map(r.rows[0]) : null;
}

function ext(name: string): string {
  const value = path.extname(name).toLowerCase();
  return /^\.[a-z0-9]{1,10}$/.test(value) ? value : '.bin';
}

export async function ingestContract(
  file: Buffer,
  fileName: string,
  mime: string,
  lineUserId: string | null,
): Promise<string> {
  const id = randomUUID();
  const key = `${prefix}${id}${ext(fileName)}`;
  await put(key, file, mime);
  try {
    const r = await query<{ id: string }>(
      `INSERT INTO law.contracts
        (id, line_user_id, file_name, file_type, file_mime, storage_bucket,
         storage_path, size_bytes, chunk_count, status, source)
       VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, 0, 'pending', $9)
       RETURNING id`,
      [
        id,
        lineUserId,
        fileName,
        ext(fileName).slice(1),
        mime,
        bucket,
        key,
        file.length,
        lineUserId ? 'line' : 'web',
      ],
    );
    return r.rows[0].id;
  } catch (err) {
    await remove(key);
    throw err;
  }
}

export async function deleteContract(id: string): Promise<boolean> {
  const current = await query<{
    storage_path: string | null;
    page_index: number | null;
    image_mime: string | null;
  }>(
    `SELECT c.storage_path, p.page_index, p.image_mime
       FROM law.contracts c
       LEFT JOIN law.contract_pages p ON p.contract_id = c.id
      WHERE c.id = $1::uuid`,
    [id],
  );
  if (current.rows.length === 0) return false;

  const deleted = await query(
    `DELETE FROM law.contracts
      WHERE id = $1::uuid
      RETURNING id`,
    [id],
  );
  if (deleted.rows.length === 0) return false;

  const keys = new Set<string>();
  const fileKey = current.rows[0].storage_path;
  if (fileKey?.startsWith(prefix)) keys.add(fileKey);
  for (const row of current.rows) {
    if (row.page_index == null) continue;
    const pageExt = row.image_mime === 'image/png' ? 'png' : 'jpg';
    keys.add(`${prefix}${id}/pages/${row.page_index}.${pageExt}`);
  }
  await Promise.all([...keys].map((key) => remove(key)));
  return true;
}

export async function failContract(id: string, error: string): Promise<void> {
  await query(
    `UPDATE law.contracts
        SET status = 'failed', error_message = $2
      WHERE id = $1::uuid`,
    [id, error.slice(0, 2000)],
  );
}
