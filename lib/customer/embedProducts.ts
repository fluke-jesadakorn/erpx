import 'server-only';
import { aiInvoke } from '@/ai/router';
import { query } from '../db';

const EMBED_MODEL = process.env.OLLAMA_EMBED_MODEL || process.env.EMBED_MODEL || 'bge-m3';

export async function embedAndStoreCustomer(customerId: number): Promise<void> {
  const r = await query<{ id: number; name: string; name_th: string | null }>(
    `SELECT id, name, name_th FROM folio.customers WHERE id = $1`,
    [customerId]
  );
  if (r.rows.length === 0) return;
  const c = r.rows[0];
  const text = `${c.name} ${c.name_th ?? ''}`.trim();
  if (!text) return;
  const emb = await aiInvoke('acct:coa-search', 'embed', { text, modelOverride: EMBED_MODEL });
  if (!emb.ok || !emb.embedding) return;
  const vec = `[${emb.embedding.join(',')}]`;
  await query(
    `UPDATE folio.customers SET embedding = $1::vector WHERE id = $2`,
    [vec, c.id]
  );
}

export async function embedAndStoreSoItem(soItemId: number): Promise<void> {
  const r = await query<{ id: number; description: string }>(
    `SELECT id, description FROM folio.so_items WHERE id = $1`,
    [soItemId]
  );
  if (r.rows.length === 0) return;
  const it = r.rows[0];
  if (!it.description) return;
  const emb = await aiInvoke('acct:coa-search', 'embed', { text: it.description, modelOverride: EMBED_MODEL });
  if (!emb.ok || !emb.embedding) return;
  const vec = `[${emb.embedding.join(',')}]`;
  await query(
    `INSERT INTO folio.sales_product_embeddings (so_item_id, description, embedding)
     VALUES ($1, $2, $3::vector)`,
    [it.id, it.description, vec]
  );
}

export async function searchProductsSemantic(q: string, limit = 10): Promise<Array<{ id: number; so_item_id: number; description: string; score: number }>> {
  const text = q.trim();
  if (!text) return [];
  const emb = await aiInvoke('acct:coa-search', 'embed', { text, modelOverride: EMBED_MODEL });
  if (!emb.ok || !emb.embedding) return [];
  const vec = `[${emb.embedding.join(',')}]`;
  const r = await query<{ id: number; so_item_id: number; description: string; score: string | number }>(
    `SELECT id, so_item_id, description,
            1 - (embedding <=> $1::vector) AS score
       FROM folio.sales_product_embeddings
      WHERE embedding IS NOT NULL
   ORDER BY embedding <=> $1::vector
      LIMIT $2`,
    [vec, Math.min(Math.max(limit, 1), 50)]
  );
  return r.rows.map(row => ({
    id: Number(row.id),
    so_item_id: Number(row.so_item_id),
    description: row.description,
    score: Number(row.score),
  }));
}
