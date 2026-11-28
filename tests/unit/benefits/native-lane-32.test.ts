import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

import {
  createBenefitPayrollInputsClient,
  nativeBenefitsLane32Configuration,
  readBenefitPayrollInputsReply,
  type PayrollInputArgs,
} from '../../../modules/benefits/server/src/adapters/native-lane-32/client';

const configRaw = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44994',
  host: 'acme.example.com',
  holdingSlug: 'acme',
  origin: 'https://acme.example.com',
  timeoutMs: 1000,
  companyId: 'company-1',
});
const args: PayrollInputArgs = {
  engagementId: 'engagement-1',
  currency: 'THB',
  periodEnd: '2026-12-31',
  periodStart: '2026-01-01',
};
const row = {
  id: 'input-1',
  plan_name: 'Family plan',
  employee_deduction_minor: 100,
  employer_cost_minor: 200,
  source_election_version: 3,
};
const ok = (value: unknown) =>
  Promise.resolve({ status: 200, value });

const SOURCE_SQL = "SELECT b.*,p.name plan_name FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=? AND b.currency=? AND b.status='released' AND b.effective_from<=? AND (b.effective_to IS NULL OR b.effective_to>=?) ORDER BY b.id";
const EXECUTED_SQL = "SELECT b.id,p.name plan_name,b.employee_deduction_minor::bigint employee_deduction_minor,b.employer_cost_minor::bigint employer_cost_minor,b.source_election_version::bigint source_election_version FROM benefit_payroll_inputs b JOIN benefit_elections e ON e.id=b.election_id JOIN benefit_plans p ON p.id=e.plan_id WHERE b.engagement_id=$1 AND b.currency=$2 AND b.status='released' AND b.effective_from<=$3 AND (b.effective_to IS NULL OR b.effective_to>=$4) AND b.company_id=$5 ORDER BY b.id";
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

const fails = (run: () => unknown, code: string) => {
  try {
    run();
  } catch (error) {
    expect(error).toMatchObject({ status: 503, code });
    return;
  }
  throw new Error(`expected ApiError ${code}`);
};

describe('native lane-32 Benefits payroll-input client', () => {
  it('accepts only the registered private target and Company scope', () => {
    const cfg = nativeBenefitsLane32Configuration(configRaw);
    expect(cfg).toMatchObject({
      base: 'http://127.0.0.1:44994',
      host: 'acme.example.com',
      holdingSlug: 'acme',
      origin: 'https://acme.example.com',
      companyId: 'company-1',
    });
    fails(() => nativeBenefitsLane32Configuration(''), 'BENEFIT_PAYROLL_INPUT_READ_UNAVAILABLE');
    for (const raw of [
      'not json',
      JSON.stringify({ ...JSON.parse(configRaw), privateBaseUrl: 'http://127.0.0.1:3100' }),
      JSON.stringify({ ...JSON.parse(configRaw), privateBaseUrl: 'http://localhost:44994' }),
      JSON.stringify({ ...JSON.parse(configRaw), host: 'other.example.com' }),
      JSON.stringify({ ...JSON.parse(configRaw), holdingSlug: 'ACME' }),
      JSON.stringify({ ...JSON.parse(configRaw), timeoutMs: 249 }),
    ]) {
      fails(() => nativeBenefitsLane32Configuration(raw), 'BENEFIT_PAYROLL_INPUT_READ_CONFIG_INVALID');
    }
  });

  it('forwards bounded identities only and pins the strict response shape', async () => {
    const transport = vi.fn(
      (_url: URL, _headers: Record<string, string>, _timeoutMs: number) => ok({ inputs: [row] }),
    );
    const result = await createBenefitPayrollInputsClient(
      nativeBenefitsLane32Configuration(configRaw),
      transport,
    )(args);
    expect(result).toEqual([row]);
    expect(transport).toHaveBeenCalledTimes(1);
    const [url, headers, timeoutMs] = transport.mock.calls[0]!;
    expect(String(url)).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/benefits/payroll-inputs');
    expect(headers).toMatchObject({
      host: 'acme.example.com',
      origin: 'https://acme.example.com',
      'x-erpx-context-company': 'company-1',
      'x-erpx-benefits-engagement': 'engagement-1',
      'x-erpx-benefits-currency': 'THB',
      'x-erpx-benefits-period-start': '2026-01-01',
      'x-erpx-benefits-period-end': '2026-12-31',
    });
    expect(timeoutMs).toBe(1000);
    expect(JSON.stringify(Object.keys(headers).sort())).toBe(JSON.stringify([
      'accept', 'cache-control', 'host', 'origin',
      'x-erpx-benefits-currency', 'x-erpx-benefits-engagement',
      'x-erpx-benefits-period-end', 'x-erpx-benefits-period-start',
      'x-erpx-context-company',
    ].sort()));
  });

  it('fails closed on an unavailable, malformed or widened reply', async () => {
    const cfg = nativeBenefitsLane32Configuration(configRaw);
    await expect(
      createBenefitPayrollInputsClient(cfg, () => Promise.reject(new Error('down')))(args),
    ).rejects.toMatchObject({ status: 503, code: 'BENEFIT_PAYROLL_INPUT_READ_UNAVAILABLE' });
    await expect(
      createBenefitPayrollInputsClient(cfg, () => Promise.resolve({ status: 503, value: {} }))(args),
    ).rejects.toMatchObject({ status: 503 });
    for (const value of [
      {},
      { inputs: [{ ...row, employer_cost_minor: '200' }] },
      { inputs: [row], extra: true },
    ]) {
      fails(() => readBenefitPayrollInputsReply(value), 'BENEFIT_PAYROLL_INPUT_READ_RESPONSE_INVALID');
    }
    await expect(
      createBenefitPayrollInputsClient(cfg, () => ok({ inputs: [row] }))({
        ...args,
        currency: 'thb',
      }),
    ).rejects.toMatchObject({ status: 503, code: 'BENEFIT_PAYROLL_INPUT_READ_INVALID' });
  });
});

describe('native lane-32 SQL/result oracle and TS cutover', () => {
  it('pins the retired SQL, the executed SQL and the result shape hashes', () => {
    expect(sha(SOURCE_SQL)).toBe('eccc606856a063879e8afbe293297384cfa49889e602454d7a7c7b7bff62069b');
    expect(sha(EXECUTED_SQL)).toBe('9dafd1ee920459903800aa1d04ec59441353362188d25ee11db8b64cadd3eb10');
    expect(sha(JSON.stringify({ inputs: [row] }))).toBe('443cf036deecef00a14d38e6b2a7d4a9f90cd60e83ae15487fb3557e2491fd60');
    expect(EXECUTED_SQL).toContain('b.company_id=$5');
    expect(EXECUTED_SQL).not.toContain('b.*');
  });

  it('removes the retired SQL from every switched owner file', async () => {
    const files = [
      '../../../modules/benefits/server/src/adapters/benefits-read-sql.ts',
      '../../../modules/benefits/server/src/adapters/payroll-owner-facts.ts',
      '../../../modules/benefits/server/src/adapters/payroll-source-facts.ts',
    ];
    for (const file of files) {
      const source = await readFile(new URL(file, import.meta.url), 'utf8');
      expect(source).not.toContain(SOURCE_SQL);
      expect(source).not.toContain('plan_name FROM benefit_payroll_inputs');
    }
    const owner = await readFile(
      new URL('../../../modules/benefits/server/src/adapters/payroll-owner-facts.ts', import.meta.url),
      'utf8',
    );
    expect(owner).toContain("from './native-lane-32/client'");
    expect(owner).not.toContain('.prepare(');
  });

  it('routes both live call sites through the native client with no local SQL', async () => {
    const read = vi.fn(async () => ({ results: [row] }));
    vi.resetModules();
    vi.doMock('../../../modules/benefits/server/src/adapters/native-lane-32/client', () => ({
      benefitPayrollInputs: read,
    }));
    try {
      const { payrollBenefitOwnerFacts } = await import(
        '../../../modules/benefits/server/src/adapters/payroll-owner-facts'
      );
      const { payrollBenefitSources } = await import(
        '../../../modules/benefits/server/src/adapters/payroll-source-facts'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const expected = { engagementId: 'engagement-1', currency: 'THB', periodEnd: '2026-12-31', periodStart: '2026-01-01' };
      expect(await payrollBenefitOwnerFacts(db).readCalculateItemBenefitInputs(
        'engagement-1', 'THB', '2026-12-31', '2026-01-01',
      )).toEqual({ results: [row] });
      expect(read).toHaveBeenCalledWith(db, expected, undefined);
      expect(await payrollBenefitSources(db).readSourceBlockersBenefits(
        'engagement-1', 'THB', '2026-12-31', '2026-01-01',
      )).toEqual({ results: [row] });
      expect(read).toHaveBeenCalledWith(db, expected, undefined);
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/benefits/server/src/adapters/native-lane-32/client');
      vi.resetModules();
    }
  });
});
