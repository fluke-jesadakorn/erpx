import 'server-only';

import { query } from '@folio-lib/db';
import { presignedGetUrl } from '@folio-lib/slips/storage';

export interface ContractStats {
  total: number;
  ready: number;
  pending: number;
  failed: number;
  processing: number;
  chunks: number;
  bytes: number;
}

export async function getContractStats(): Promise<ContractStats> {
  const r = await query<{
    total: string | number;
    ready: string | number;
    pending: string | number;
    failed: string | number;
    processing: string | number;
    chunks: string | number;
    bytes: string | number;
  }>(
    `SELECT COUNT(*) AS total,
            COUNT(*) FILTER (WHERE status = 'ready') AS ready,
            COUNT(*) FILTER (WHERE status = 'pending') AS pending,
            COUNT(*) FILTER (WHERE status = 'failed') AS failed,
            COUNT(*) FILTER (WHERE status = 'processing') AS processing,
            (SELECT COUNT(*) FROM law.contract_chunks) AS chunks,
            COALESCE(SUM(size_bytes), 0) AS bytes
       FROM law.contracts`,
  );
  const row = r.rows[0];
  return {
    total: Number(row.total),
    ready: Number(row.ready),
    pending: Number(row.pending),
    failed: Number(row.failed),
    processing: Number(row.processing),
    chunks: Number(row.chunks),
    bytes: Number(row.bytes),
  };
}

export async function previewContract(
  id: string,
): Promise<{ pdfUrl: string; pages: string[] }> {
  const contract = await query<{ storage_path: string | null }>(
    `SELECT storage_path
       FROM law.contracts
      WHERE id = $1::uuid`,
    [id],
  );
  if (contract.rows.length === 0) throw new Error('Contract not found');

  const pageRows = await query<{ page_index: number; image_mime: string }>(
    `SELECT page_index, image_mime
       FROM law.contract_pages
      WHERE contract_id = $1::uuid
      ORDER BY page_index`,
    [id],
  );
  const key = contract.rows[0].storage_path;
  const pdfUrl = key?.startsWith('law/contracts/')
    ? await presignedGetUrl(key)
    : '';
  const pages = await Promise.all(
    pageRows.rows.map((page) => {
      const ext = page.image_mime === 'image/png' ? 'png' : 'jpg';
      return presignedGetUrl(`law/contracts/${id}/pages/${page.page_index}.${ext}`);
    }),
  );
  return { pdfUrl, pages };
}
