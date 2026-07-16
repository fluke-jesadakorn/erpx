import 'server-only';

import { query } from '@folio-lib/db';
import { invoke } from '@folio-lib/ai/router';
import { enqueueIndexing } from './queue';

export interface ContractChunk {
  id: string;
  contractId: string;
  chunkIndex: number;
  content: string;
  tokenCount: number | null;
  metadata: unknown;
  createdAt: string;
  score?: number;
  docNo?: string | null;
  fileName?: string;
}

interface ChunkRow {
  id: string;
  contract_id: string;
  chunk_index: number;
  content: string;
  token_count: number | null;
  metadata: unknown;
  created_at: Date | string;
}

function map(row: ChunkRow): ContractChunk {
  return {
    id: row.id,
    contractId: row.contract_id,
    chunkIndex: Number(row.chunk_index),
    content: row.content,
    tokenCount: row.token_count == null ? null : Number(row.token_count),
    metadata: row.metadata,
    createdAt: new Date(row.created_at).toISOString(),
  };
}

export function chunkText(text: string, size = 512, overlap = 64): string[] {
  const width = Math.max(1, Math.floor(size));
  const shared = Math.min(Math.max(0, Math.floor(overlap)), width - 1);
  const value = text.replace(/\r\n/g, '\n').trim();
  if (!value) return [];
  const chunks: string[] = [];
  for (let start = 0; start < value.length; start += width - shared) {
    const chunk = value.slice(start, start + width).trim();
    if (chunk) chunks.push(chunk);
    if (start + width >= value.length) break;
  }
  return chunks;
}

export async function embedChunk(text: string): Promise<number[]> {
  const model = process.env.OLLAMA_EMBED_MODEL || process.env.EMBED_MODEL || 'bge-m3';
  const r = await invoke('law:contracts', 'embed', { text, modelOverride: model });
  if (!r.ok || !r.embedding) throw new Error(r.error || 'Embedding failed');
  if (r.embedding.length !== 1024) {
    throw new Error(`Expected 1024 embedding dimensions, received ${r.embedding.length}`);
  }
  return r.embedding;
}

export async function listChunks(contractId: string, limit = 500): Promise<ContractChunk[]> {
  const safeLimit = Math.min(Math.max(Math.floor(limit), 1), 2000);
  const r = await query<ChunkRow>(
    `SELECT id, contract_id, chunk_index, content, token_count, metadata, created_at
       FROM law.contract_chunks
      WHERE contract_id = $1::uuid
      ORDER BY chunk_index
      LIMIT $2`,
    [contractId, safeLimit],
  );
  return r.rows.map(map);
}

export async function indexContractText(contractId: string, text: string): Promise<number> {
  await enqueueIndexing({ contractId, rawText: text });
  const existing = await query<{ chunk_count: number | null }>(
    `SELECT chunk_count FROM law.contracts WHERE id = $1::uuid`,
    [contractId],
  );
  return existing.rows[0]?.chunk_count ?? 0;
}

export async function runIndexingJob(_jobId: string, contractId: string, text: string | null): Promise<number> {
  const t = text ?? '';
  if (!t) {
    await query(
      `UPDATE law.contracts
          SET status = 'failed', error_message = 'no text provided'
        WHERE id = $1::uuid`,
      [contractId],
    );
    throw new Error('no text provided');
  }
  const chunks = chunkText(t);
  try {
    const rows: Array<{
      chunk_index: number;
      content: string;
      token_count: number;
      embedding: string;
      metadata: Record<string, never>;
    }> = [];
    for (let i = 0; i < chunks.length; i += 1) {
      const embedding = await embedChunk(chunks[i]);
      rows.push({
        chunk_index: i,
        content: chunks[i],
        token_count: Math.ceil(chunks[i].length / 4),
        embedding: `[${embedding.join(',')}]`,
        metadata: {},
      });
    }

    if (rows.length > 0) {
      await query(
        `INSERT INTO law.contract_chunks
          (contract_id, chunk_index, content, token_count, embedding, metadata)
         SELECT $1::uuid, x.chunk_index, x.content, x.token_count,
                x.embedding::vector, x.metadata
           FROM jsonb_to_recordset($2::jsonb) AS x(
             chunk_index integer,
             content text,
             token_count integer,
             embedding text,
             metadata jsonb
           )
         ON CONFLICT (contract_id, chunk_index) DO UPDATE
           SET content = EXCLUDED.content,
               token_count = EXCLUDED.token_count,
               embedding = EXCLUDED.embedding,
               metadata = EXCLUDED.metadata`,
        [contractId, JSON.stringify(rows)],
      );
    }

    await query(
      `DELETE FROM law.contract_chunks
        WHERE contract_id = $1::uuid
          AND chunk_index >= $2`,
      [contractId, rows.length],
    );
    await query(
      `UPDATE law.contracts
          SET status = 'ready', chunk_count = $2, error_message = NULL
        WHERE id = $1::uuid`,
      [contractId, rows.length],
    );
    return rows.length;
  } catch (err) {
    await query(
      `UPDATE law.contracts
          SET status = 'failed', error_message = $2
        WHERE id = $1::uuid`,
      [contractId, (err as Error).message.slice(0, 2000)],
    ).catch(() => {});
    throw err;
  }
}
