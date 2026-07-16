import 'server-only';
import { query } from '../db';
import { aiInvoke } from '@folio-lib/ai/router';
import { STAGE_ORDER } from '../perm/stages';

export interface PolicyLintFinding {
  code: string;
  severity: 'error' | 'warning' | 'info';
  message: string;
}

export interface PolicyLintResult {
  policyId: string;
  policyName: string;
  findings: PolicyLintFinding[];
  generatedAt: string;
}

function systemPrompt(): string {
  return `You review an approval-policy AST for internal contradictions, unreachable stages, and gaps in the chain. The valid finance-standard stages are: ${STAGE_ORDER.join(', ')}. Reply with JSON only: {"findings":[{"code":"...","severity":"error|warning|info","message":"..."}]}. If the policy looks correct, return {"findings":[]}.`;
}

function safeParse(s: string): { findings: PolicyLintFinding[] } | null {
  const m = s.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try { return JSON.parse(m[0]); } catch { return null; }
}

export async function lintPolicy(policyId: string): Promise<PolicyLintResult | null> {
  const r = await query<{ id: string; name: string; ast: unknown }>(
    `SELECT id, name, ast FROM perm.policies WHERE id = $1 AND enabled = TRUE`,
    [policyId]
  );
  if (r.rows.length === 0) return null;
  const policy = r.rows[0];

  const resp = await aiInvoke('policy:editor', 'chat', {
    systemPrompt: systemPrompt(),
    text: JSON.stringify({ id: policy.id, name: policy.name, ast: policy.ast }),
    temperature: 0.1,
    maxTokens: 800,
  });
  if (!resp.ok || !resp.text) return null;

  const parsed = safeParse(resp.text);
  const findings = Array.isArray(parsed?.findings)
    ? parsed.findings
        .filter((f: any) => f && typeof f.code === 'string' && typeof f.message === 'string')
        .map((f: any) => ({
          code: String(f.code),
          severity: (['error', 'warning', 'info'].includes(f.severity) ? f.severity : 'info') as PolicyLintFinding['severity'],
          message: String(f.message),
        }))
    : [];

  return {
    policyId: policy.id,
    policyName: policy.name,
    findings,
    generatedAt: new Date().toISOString(),
  };
}

export async function lintAllPolicies(): Promise<PolicyLintResult[]> {
  const ids = await query<{ id: string }>(`SELECT id FROM perm.policies WHERE enabled = TRUE ORDER BY id`);
  const out: PolicyLintResult[] = [];
  for (const row of ids.rows) {
    const r = await lintPolicy(row.id);
    if (r) out.push(r);
  }
  return out;
}