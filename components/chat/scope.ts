import type { QuickPrompt } from './quickPrompts';
import { QUICK_PROMPTS } from './quickPrompts';

export interface ChatScope {
  tileId: string;
  displayName: string;
  hint: string;
  quickPrompts: QuickPrompt[];
  sectionKey: string;
}

export function deriveScope(pathname: string, search: string): ChatScope | null {
  if (pathname === '/login') return null;
  if (pathname === '/chat') return null;

  const sp = new URLSearchParams(search);
  const scopeParam = sp.get('scope') ?? sp.get('tab');

  const fallback: ChatScope = {
    tileId: 'global',
    displayName: 'Folio',
    hint: 'You have access to all allow-listed tables.',
    quickPrompts: QUICK_PROMPTS['cockpit'] ?? [],
    sectionKey: 'chat:global',
  };

  const make = (tileId: string, displayName: string, hint: string, qpKey: string): ChatScope => ({
    tileId,
    displayName,
    hint,
    quickPrompts: QUICK_PROMPTS[qpKey] ?? QUICK_PROMPTS['cockpit'] ?? [],
    sectionKey: `chat:${tileId}`,
  });

  if (pathname === '/') return make('hub', 'Hub', 'You are on the Hub home page.', 'hub');
  if (pathname === '/expense') {
    const tail = scopeParam ? ` · ${scopeParam}` : '';
    return make('expense', `Expense${tail}`, 'You are on the Expense page.', 'expense');
  }
  if (pathname === '/sales' || pathname.startsWith('/sales/')) {
    return make('sales', 'Sales', 'You are on the Sales page.', 'sales');
  }
  if (pathname === '/customers' || pathname.startsWith('/customers/')) {
    return make('customers', 'Customers', 'You are on the Customers page.', 'customers');
  }
  if (pathname === '/pr' || pathname.startsWith('/pr/')) return make('pr', 'PR', 'You are on the PR page.', 'pr');
  if (pathname === '/po' || pathname.startsWith('/po/')) return make('po', 'PO', 'You are on the PO page.', 'po');
  if (pathname === '/cockpit' || pathname.startsWith('/cockpit/')) return make('cockpit', 'Cockpit', 'You are on the Cockpit page.', 'cockpit');
  if (pathname === '/executive' || pathname.startsWith('/executive/')) return make('executive', 'Executive finance', 'Use posted accounting, controlled subledgers, inventory, and operational pipeline data for executive decisions.', 'executive');
  if (pathname === '/ledger' || pathname.startsWith('/ledger/')) return make('ledger', 'Ledger', 'You are on the Ledger page.', 'ledger');
  if (pathname === '/policy' || pathname.startsWith('/policy/')) return make('policy', 'Policy', 'You are on the Policy page.', 'policy');
  if (pathname === '/tiles' || pathname.startsWith('/tiles/')) return make('tiles', 'Tiles', 'You are on the Tile Catalog page.', 'tiles');
  if (pathname === '/audit' || pathname.startsWith('/audit/')) return make('audit', 'Audit', 'You are on the Audit page.', 'audit');
  if (pathname === '/ai-settings' || pathname.startsWith('/ai-settings/')) return make('ai-settings', 'AI Settings', 'You are on the AI Settings page.', 'ai-settings');
  if (pathname === '/hr' || pathname.startsWith('/hr/')) return make('hr', 'HR', 'You are on the HR page.', 'hr');
  if (pathname === '/law' || pathname.startsWith('/law/')) return make('law', 'Law', 'You are on the Law page.', 'law');
  if (pathname === '/org-chart' || pathname.startsWith('/org-chart/')) return make('org', 'Org', 'You are on the Org Chart page.', 'org');
  if (pathname === '/inbox' || pathname.startsWith('/inbox/')) return make('inbox', 'Inbox', 'You are on the Inbox page.', 'inbox');
  if (pathname === '/inbox' || pathname.startsWith('/inbox/')) {
    const tail = scopeParam ? ` · ${scopeParam}` : '';
    return make('waybills', `Waybills${tail}`, 'You are on the Waybills page.', 'waybills');
  }
  if (pathname.startsWith('/waybill/')) {
    const id = pathname.split('/')[2] ?? '';
    const short = id.length > 10 ? id.slice(0, 10) + '…' : id;
    return make('waybill', `Waybill ${short}`, 'You are viewing a specific waybill.', 'waybill');
  }

  return fallback;
}
