// Safe interactive UI contract.
// Model emits a [UI]{...}[/UI] block describing a tree of allow-listed
// components. The renderer never executes model-supplied JavaScript — every
// interactive action is bound to a server-owned action id mapped in
// lib/chat/safeUiActions.ts.

export const UI_BLOCK = /\[UI\]([\s\S]*?)\[\/UI\]/g;

export type UiNode =
  | UiRoot
  | UiStack
  | UiGrid
  | UiCard
  | UiHeading
  | UiText
  | UiMetric
  | UiTable
  | UiTabs
  | UiAccordion
  | UiButton
  | UiNote;

export interface UiBase {
  type: string;
  id?: string;
}

export interface UiRoot extends UiBase {
  type: 'root';
  children: UiNode[];
}
export interface UiStack extends UiBase {
  type: 'stack';
  direction?: 'row' | 'col';
  gap?: number;
  children: UiNode[];
}
export interface UiGrid extends UiBase {
  type: 'grid';
  columns?: number;
  gap?: number;
  children: UiNode[];
}
export interface UiCard extends UiBase {
  type: 'card';
  title?: string;
  children: UiNode[];
}
export interface UiHeading extends UiBase {
  type: 'heading';
  level?: 1 | 2 | 3;
  text: string;
}
export interface UiText extends UiBase {
  type: 'text';
  text: string;
}
export interface UiMetric extends UiBase {
  type: 'metric';
  label: string;
  value: string;
  hint?: string;
  tone?: 'neutral' | 'positive' | 'negative';
}
export interface UiTable extends UiBase {
  type: 'table';
  columns: string[];
  rows: Array<Array<string | number>>;
  caption?: string;
}
export interface UiTabs extends UiBase {
  type: 'tabs';
  tabs: Array<{ id: string; label: string; children: UiNode[] }>;
  active?: string;
}
export interface UiAccordion extends UiBase {
  type: 'accordion';
  items: Array<{ id: string; title: string; children: UiNode[] }>;
}
export interface UiButton extends UiBase {
  type: 'button';
  label: string;
  action: string;
  payload?: Record<string, unknown>;
  variant?: 'primary' | 'secondary' | 'danger';
}
export interface UiNote extends UiBase {
  type: 'note';
  tone?: 'info' | 'warn' | 'danger';
  text: string;
}

const ALLOWED_TYPES = new Set<UiNode['type']>([
  'root', 'stack', 'grid', 'card', 'heading', 'text', 'metric',
  'table', 'tabs', 'accordion', 'button', 'note',
]);

function asNode(v: unknown, depth = 0): UiNode | null {
  if (depth > 6) return null;
  if (!v || typeof v !== 'object') return null;
  const o = v as Record<string, unknown>;
  const t = String(o.type ?? '');
  if (!ALLOWED_TYPES.has(t as UiNode['type'])) return null;
  switch (t) {
    case 'root':
    case 'stack':
    case 'grid':
    case 'card':
      return { type: t as UiNode['type'], id: typeof o.id === 'string' ? o.id : undefined, children: asNodes(o.children, depth + 1) } as UiNode;
    case 'heading':
      return { type: 'heading', level: ([1, 2, 3].includes(Number(o.level)) ? Number(o.level) : 2) as 1 | 2 | 3, text: String(o.text ?? '') } as UiNode;
    case 'text':
      return { type: 'text', text: String(o.text ?? '') } as UiNode;
    case 'metric':
      return {
        type: 'metric',
        label: String(o.label ?? ''),
        value: String(o.value ?? ''),
        hint: typeof o.hint === 'string' ? o.hint : undefined,
        tone: o.tone === 'positive' || o.tone === 'negative' ? o.tone : 'neutral',
      } as UiNode;
    case 'table':
      return {
        type: 'table',
        columns: Array.isArray(o.columns) ? o.columns.map(String) : [],
        rows: Array.isArray(o.rows)
          ? o.rows.slice(0, 200).map((r: unknown) => Array.isArray(r) ? r.slice(0, 50).map((c: unknown) => typeof c === 'number' || typeof c === 'string' ? c : String(c ?? '')) : [])
          : [],
        caption: typeof o.caption === 'string' ? o.caption : undefined,
      } as UiNode;
    case 'tabs':
      return {
        type: 'tabs',
        active: typeof o.active === 'string' ? o.active : undefined,
        tabs: Array.isArray(o.tabs)
          ? o.tabs.slice(0, 12).map((tab: unknown): { id: string; label: string; children: UiNode[] } => {
              const t = (tab ?? {}) as Record<string, unknown>;
              return {
                id: typeof t.id === 'string' ? t.id : `tab_${Math.random().toString(36).slice(2, 7)}`,
                label: String(t.label ?? ''),
                children: asNodes(t.children, depth + 1),
              };
            })
          : [],
      } as UiNode;
    case 'accordion':
      return {
        type: 'accordion',
        items: Array.isArray(o.items)
          ? o.items.slice(0, 12).map((it: unknown): { id: string; title: string; children: UiNode[] } => {
              const i = (it ?? {}) as Record<string, unknown>;
              return {
                id: typeof i.id === 'string' ? i.id : `acc_${Math.random().toString(36).slice(2, 7)}`,
                title: String(i.title ?? ''),
                children: asNodes(i.children, depth + 1),
              };
            })
          : [],
      } as UiNode;
    case 'button':
      return {
        type: 'button',
        id: typeof o.id === 'string' ? o.id : undefined,
        label: String(o.label ?? '').slice(0, 80),
        action: String(o.action ?? '').slice(0, 80),
        payload: o.payload && typeof o.payload === 'object' ? (o.payload as Record<string, unknown>) : undefined,
        variant: o.variant === 'primary' || o.variant === 'danger' ? o.variant : 'secondary',
      } as UiNode;
    case 'note':
      return {
        type: 'note',
        text: String(o.text ?? ''),
        tone: o.tone === 'warn' || o.tone === 'danger' ? o.tone : 'info',
      } as UiNode;
  }
  return null;
}

function asNodes(v: unknown, depth: number): UiNode[] {
  if (!Array.isArray(v)) return [];
  const out: UiNode[] = [];
  for (const item of v) {
    const n = asNode(item, depth);
    if (n) out.push(n);
  }
  return out;
}

export interface ParsedUi { root: UiRoot | null; raw: string | null; }

export function parseUiBlock(body: string): ParsedUi {
  const trimmed = body.trim();
  if (!trimmed) return { root: null, raw: null };
  let parsed: unknown;
  try {
    parsed = JSON.parse(trimmed);
  } catch {
    return { root: null, raw: trimmed };
  }
  const node = asNode(parsed, 0);
  if (node?.type === 'root') return { root: node, raw: trimmed };
  if (node) return { root: { type: 'root', children: [node] }, raw: trimmed };
  return { root: null, raw: trimmed };
}

export function parseUiBlocks(text: string): { plain: string; blocks: ParsedUi[] } {
  const blocks: ParsedUi[] = [];
  if (!text) return { plain: '', blocks };
  const plain = text.replace(UI_BLOCK, (_m, body: string) => {
    const p = parseUiBlock(body);
    if (p.root) blocks.push(p);
    return '';
  });
  return { plain: plain.trim(), blocks };
}