import 'server-only';

import { query } from '@folio-lib/db';
import { invoke } from '@folio-lib/ai/router';
import { embedChunk, type ContractChunk } from './chunks';

interface SearchRow {
  id: string;
  contract_id: string;
  chunk_index: number;
  content: string;
  token_count: number | null;
  metadata: unknown;
  created_at: Date | string;
  score: string | number;
  doc_no: string | null;
  file_name: string;
}

export async function search(queryText: string, k = 5): Promise<ContractChunk[]> {
  const text = queryText.trim();
  if (!text) return [];
  const limit = Math.min(Math.max(Math.floor(k), 1), 20);
  const embedding = await embedChunk(text);
  const vector = `[${embedding.join(',')}]`;
  const r = await query<SearchRow>(
    `SELECT ch.id, ch.contract_id, ch.chunk_index, ch.content, ch.token_count,
            ch.metadata, ch.created_at,
            1 - (ch.embedding <=> $1::vector) AS score,
            c.doc_no, c.file_name
       FROM law.contract_chunks ch
       JOIN law.contracts c ON c.id = ch.contract_id
      WHERE ch.embedding IS NOT NULL
        AND c.status = 'ready'
      ORDER BY ch.embedding <=> $1::vector
      LIMIT $2`,
    [vector, limit],
  );
  return r.rows.map((row) => ({
    id: row.id,
    contractId: row.contract_id,
    chunkIndex: Number(row.chunk_index),
    content: row.content,
    tokenCount: row.token_count == null ? null : Number(row.token_count),
    metadata: row.metadata,
    createdAt: new Date(row.created_at).toISOString(),
    score: Number(row.score),
    docNo: row.doc_no,
    fileName: row.file_name,
  }));
}

export async function ask(
  queryText: string,
): Promise<{ answer: string; sources: ContractChunk[] }> {
  const text = queryText.trim();
  if (!text) throw new Error('Query is required');
  const sources = await search(text, 5);
  if (sources.length === 0) {
    return {
      answer: 'ไม่พบข้อมูลที่เกี่ยวข้องในเอกสารกฎหมายที่พร้อมใช้งาน',
      sources,
    };
  }

  const context = sources
    .map((source, i) => {
      const label = source.docNo || source.fileName || source.contractId;
      return `[${i + 1}] ${label}#${source.chunkIndex}\n${source.content}`;
    })
    .join('\n\n---\n\n');
  const model = process.env.OLLAMA_AGENT_MODEL || 'qwen3.6:35b-a3b-q4_K_M';
  const r = await invoke('law:rag', 'chat', {
    modelOverride: model,
    temperature: 0.1,
    systemPrompt: 'คุณเป็นผู้ช่วยด้านเอกสารกฎหมาย ตอบเป็นภาษาไทยโดยใช้เฉพาะบริบทที่ให้มา หากบริบทไม่เพียงพอให้ระบุว่าไม่พบข้อมูล และอ้างอิงแหล่งที่มาในรูป [เลข] เสมอ',
    text: `บริบท:\n${context}\n\nคำถาม: ${text}`,
  });
  if (!r.ok || !r.text) throw new Error(r.error || 'RAG answer failed');
  return { answer: r.text, sources };
}
