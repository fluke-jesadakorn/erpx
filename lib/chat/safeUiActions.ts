// Server-owned action registry for the safe interactive UI.
// Model emits a UiButton with `action: '<name>'`; this map binds that name to
// either a built-in client-side handler (e.g., navigate, copy) or a server
// endpoint that must be invoked via fetch with the actor's session.
//
// Unknown action ids are dropped at render time — never executed.

export type SafeUiActionKind = 'navigate' | 'copy' | 'api' | 'noop';

export interface SafeUiActionSpec {
  kind: SafeUiActionKind;
  href?: string;
  apiPath?: string;
  method?: 'GET' | 'POST';
}

const REGISTRY: Record<string, SafeUiActionSpec> = {
  refresh_cashflow: { kind: 'api', apiPath: '/api/finance/cashflow', method: 'GET' },
  view_cockpit:     { kind: 'navigate', href: '/cockpit' },
  view_inbox:       { kind: 'navigate', href: '/inbox' },
  copy_sql:         { kind: 'copy', apiPath: 'sql' },
  new_chat:         { kind: 'navigate', href: '/chat' },
  noop:             { kind: 'noop' },
};

export function resolveAction(action: string): SafeUiActionSpec | null {
  if (typeof action !== 'string' || !action) return null;
  return REGISTRY[action] ?? null;
}

export function listRegisteredActions(): string[] {
  return Object.keys(REGISTRY);
}