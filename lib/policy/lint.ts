import 'server-only';
import { query } from '../db';
import { aiInvoke } from '@/ai/router';
import { PERM_ID_REGEX } from '../perm/grammar';
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

export function lintAst(ast: unknown, policyId = 'inline', policyName = 'inline'): PolicyLintResult {
  const findings: PolicyLintFinding[] = [];
  if (ast == null || typeof ast !== 'object' || Array.isArray(ast)) {
    findings.push({ code: 'ast.not_object', severity: 'error', message: 'ast must be a JSON object' });
    return { policyId, policyName, findings, generatedAt: new Date().toISOString() };
  }
  const astObj = ast as Record<string, unknown>;
  const rulesRaw = astObj.rules;
  if (!Array.isArray(rulesRaw)) {
    findings.push({ code: 'ast.rules_missing', severity: 'error', message: 'ast.rules must be an array' });
    return { policyId, policyName, findings, generatedAt: new Date().toISOString() };
  }
  const seen = new Set<string>();
  for (let i = 0; i < rulesRaw.length; i++) {
    const r = rulesRaw[i];
    if (r == null || typeof r !== 'object' || Array.isArray(r)) {
      findings.push({ code: 'ast.rule_not_object', severity: 'error', message: `rules[${i}] must be an object` });
      continue;
    }
    const allow = (r as { allow?: unknown }).allow;
    if (typeof allow !== 'string') {
      findings.push({ code: 'ast.rule.allow_missing', severity: 'error', message: `rules[${i}].allow must be a string` });
    } else if (!PERM_ID_REGEX.test(allow)) {
      findings.push({ code: 'ast.rule.allow_invalid', severity: 'error', message: `rules[${i}].allow "${allow}" is not a valid perm id` });
    } else {
      if (seen.has(allow)) {
        findings.push({ code: 'ast.rule.allow_dup', severity: 'warning', message: `duplicate allow "${allow}"` });
      }
      seen.add(allow);
    }
    const when = (r as { when?: unknown }).when;
    if (when != null && typeof when !== 'string') {
      findings.push({ code: 'ast.rule.when_type', severity: 'warning', message: `rules[${i}].when must be a string expression` });
    }
  }
  if (rulesRaw.length === 0) {
    findings.push({ code: 'ast.rules_empty', severity: 'info', message: 'no rules defined' });
  }
  return { policyId, policyName, findings, generatedAt: new Date().toISOString() };
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
  if (!resp.ok || !resp.text) return lintAst(policy.ast, policy.id, policy.name);

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