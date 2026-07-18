import 'server-only';
import { aiInvoke } from '@/ai/router';
import { query } from '../db';
import { loadActor } from '@/server/guard';
import { getActorScope } from '@/perm/server';

export interface RagHit {
  expense_id: number;
  vendor_name: string | null;
  description: string | null;
  amount_thb: number | null;
  transaction_date: string | null;
  score: number;
}

export interface RagAnswer {
  question: string;
  answer: string;
  hits: RagHit[];
}

const EMBED_MODEL = process.env.OLLAMA_EMBED_MODEL || process.env.EMBED_MODEL || 'bge-m3';

async function embed(text: string): Promise<number[] | null> {
  const r = await aiInvoke('finance:rag', 'embed', { text, modelOverride: EMBED_MODEL });
  if (!r.ok || !r.embedding) return null;
  if (r.embedding.length !== 1024) {
    throw new Error(`Expected 1024 embedding dimensions, got ${r.embedding.length}`);
  }
  return r.embedding;
}

async function loadActorScope(): Promise<{ userId: number; whereSql: string; params: unknown[] }> {
  const actor = await loadActor();
  if (!actor) return { userId: 0, whereSql: '', params: [] };
  const scope = await getActorScope(new Set(actor.permissions ?? []), actor.id);
  if (scope.kind === 'all') return { userId: actor.id, whereSql: '', params: [] };
  if (scope.kind === 'department' && scope.deptId) {
    return {
      userId: actor.id,
      whereSql: 'AND ve.submitter_id IN (SELECT user_id FROM perm.user_permissions WHERE permission_id = $1 AND revoked_at IS NULL AND (ends_at IS NULL OR ends_at > now()))',
      params: [`user:dept:${scope.deptId}::allow`],
    };
  }
  return { userId: actor.id, whereSql: 'AND ve.submitter_id = $1', params: [actor.id] };
}

export async function embedAndStoreExpense(expenseId: number): Promise<void> {
  const r = await query<{ id: number; vendor_name: string | null; total_amount: string | null; transaction_date: Date | null; submitter_id: number | null; items_joined: string | null }>(
    `SELECT e.id, e.vendor_name, e.total_amount::text, e.transaction_date, e.submitter_id,
            (SELECT string_agg(description, ' | ' ORDER BY id) FROM folio.expense_items WHERE expense_id = e.id) AS items_joined
       FROM folio.expenses e
      WHERE e.id = $1`,
    [expenseId]
  );
  if (r.rows.length === 0) return;
  const row = r.rows[0];
  const description = row.items_joined ?? '';
  const text = `${row.vendor_name ?? ''} | ${description}`.trim();
  if (!text) return;
  const embedding = await embed(text);
  if (!embedding) return;
  const vec = `[${embedding.join(',')}]`;
  await query(
    `INSERT INTO folio.vendor_embeddings (expense_id, submitter_id, vendor_name, description, amount_thb, transaction_date, embedding)
     VALUES ($1, $2, $3, $4, $5, $6, $7::vector)`,
    [
      row.id,
      row.submitter_id,
      row.vendor_name,
      description,
      row.total_amount ? parseFloat(row.total_amount) : null,
      row.transaction_date,
      vec,
    ]
  );
}

export async function searchVendors(args: {
  query: string;
  k?: number;
  dateFrom?: string | null;
  dateTo?: string | null;
  amountMin?: number | null;
  amountMax?: number | null;
}): Promise<RagHit[]> {
  const k = Math.min(Math.max(args.k ?? 10, 1), 50);
  const embedding = await embed(args.query);
  if (!embedding) return [];
  const vec = `[${embedding.join(',')}]`;
  const ctx = await loadActorScope();

  const filters: string[] = ['ve.embedding IS NOT NULL'];
  const params: unknown[] = [vec];
  if (ctx.whereSql) {
    const base = ctx.whereSql.replace(/^AND\s+/i, '');
    const renumbered = base.replace(/\$(\d+)/g, (_, n) => `$${parseInt(n, 10) + params.length}`);
    filters.push(renumbered);
    params.push(...ctx.params);
  }
  if (args.dateFrom) { params.push(args.dateFrom); filters.push(`ve.transaction_date >= $${params.length}`); }
  if (args.dateTo) { params.push(args.dateTo); filters.push(`ve.transaction_date <= $${params.length}`); }
  if (args.amountMin != null) { params.push(args.amountMin); filters.push(`ve.amount_thb >= $${params.length}`); }
  if (args.amountMax != null) { params.push(args.amountMax); filters.push(`ve.amount_thb <= $${params.length}`); }
  params.push(k);

  const r = await query<RagHit>(
    `SELECT ve.expense_id, ve.vendor_name, ve.description, ve.amount_thb::float8 AS amount_thb,
            ve.transaction_date::text AS transaction_date,
            1 - (ve.embedding <=> $1::vector) AS score
       FROM folio.vendor_embeddings ve
      WHERE ${filters.join(' AND ')}
   ORDER BY ve.embedding <=> $1::vector
      LIMIT $${params.length}`,
    params
  );
  return r.rows.map(row => ({
    expense_id: row.expense_id,
    vendor_name: row.vendor_name,
    description: row.description,
    amount_thb: row.amount_thb,
    transaction_date: row.transaction_date,
    score: Number(row.score),
  }));
}

export async function askFinance(question: string, lang: 'en' | 'th' | 'de' = 'en'): Promise<RagAnswer | null> {
  const hits = await searchVendors({ query: question, k: 8 });
  if (hits.length === 0) {
    return {
      question,
      hits: [],
      answer: lang === 'th'
        ? 'ไม่พบข้อมูลในประวัติการเบิกจ่าย'
        : lang === 'de'
          ? 'Keine passenden Belege gefunden.'
          : 'No matching expense history found.',
    };
  }

  const context = hits.map((h, i) => {
    return `[${i + 1}] vendor=${h.vendor_name ?? '?'}, amount=${h.amount_thb ?? '?'}, date=${h.transaction_date ?? '?'}\n    ${(h.description ?? '').slice(0, 280)}`;
  }).join('\n');

  const langLine = lang === 'th'
    ? 'ตอบเป็นภาษาไทย ใช้เฉพาะข้อมูลจากบริบท อ้างอิง [n] เมื่อจำเป็น'
    : lang === 'de'
      ? 'Antworten Sie auf Deutsch. Verwenden Sie nur den bereitgestellten Kontext. Zitieren Sie [n] wenn nötig.'
      : 'Reply in English using only the context. Cite [n] when relevant.';

  const r = await aiInvoke('finance:rag', 'chat', {
    systemPrompt: `You are a Thai-ERP finance analyst. ${langLine} Keep the answer under 200 words.`,
    text: `Context:\n${context}\n\nQuestion: ${question}`,
    temperature: 0.1,
    maxTokens: 600,
  });

  if (!r.ok || !r.text) return null;
  return { question, answer: r.text, hits };
}