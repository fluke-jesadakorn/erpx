import 'server-only';
import { aiInvoke } from './router';

export interface CommandMatch {
  tileId: string;
  confidence: number;
  reason: string;
}

export interface CommandIntent {
  top: CommandMatch | null;
  alternatives: CommandMatch[];
  raw: string;
}

const TILE_CATALOG = [
  { id: 'cockpit', title: 'Cockpit', description: 'Executive KPIs, balance sheet, cashflow' },
  { id: 'expense', title: 'My Expense', description: 'Submit and track expense claims' },
  { id: 'waybill', title: 'My Waybills', description: 'Track waybill approvals and history' },
  { id: 'sales', title: 'Sales Orders', description: 'Create and manage sales orders' },
  { id: 'customers', title: 'Customers', description: 'Customer master and AR aging' },
  { id: 'policy', title: 'Approval Policies', description: 'Edit and lint approval policies' },
  { id: 'roles', title: 'Roles & Permissions', description: 'Manage roles, users, grants' },
  { id: 'tiles', title: 'Tiles', description: 'Tile catalog and visibility' },
  { id: 'audit', title: 'Audit Log', description: 'Permission and tile audit trail' },
  { id: 'law', title: 'Law Contracts', description: 'Contracts RAG search' },
  { id: 'hr', title: 'HR', description: 'Employees, leave, schedules' },
  { id: 'org_chart', title: 'Org Chart', description: 'Reporting tree and auto-wire' },
];

function systemPrompt(): string {
  return `You map a free-text user query to one of the available tiles. Reply with JSON only, no prose: {"matches":[{"tileId":"...","confidence":0.0,"reason":"..."}]}. Confidence in [0,1]. Order by descending confidence.`;
}

function safeParse(s: string): { matches?: Array<{ tileId?: string; confidence?: number; reason?: string }> } | null {
  const m = s.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
}

export async function classifyIntent(query: string): Promise<CommandIntent | null> {
  const text = query.trim();
  if (!text) return null;

  const r = await aiInvoke('command:intent', 'chat', {
    systemPrompt: systemPrompt(),
    text: JSON.stringify({ query: text, tiles: TILE_CATALOG }),
    temperature: 0,
    maxTokens: 400,
  });
  if (!r.ok || !r.text) return null;

  const parsed = safeParse(r.text);
  const validIds = new Set(TILE_CATALOG.map(t => t.id));
  const matches: CommandMatch[] = (parsed?.matches ?? [])
    .filter((m): m is { tileId: string; confidence: number; reason: string } =>
      typeof m.tileId === 'string' && validIds.has(m.tileId) &&
      Number.isFinite(m.confidence))
    .map(m => ({
      tileId: m.tileId,
      confidence: Math.max(0, Math.min(1, Number(m.confidence))),
      reason: String(m.reason ?? ''),
    }))
    .sort((a, b) => b.confidence - a.confidence)
    .slice(0, 5);

  return {
    top: matches[0] ?? null,
    alternatives: matches.slice(1),
    raw: r.text,
  };
}