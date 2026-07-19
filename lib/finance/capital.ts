import 'server-only';
import { query } from '@/db';
import { matchPerm } from '@/perm/grammar';
import { approveAndPostJournal, preparePosting, PostingError } from './journals';
import type { PostingActor } from './types';

export interface CapitalActor extends PostingActor {
  fullname: string;
  roleName: string;
  departmentId: string | null;
}

export interface CapitalAccount {
  code: string;
  name: string;
  controlType: 'bank' | 'cash' | null;
}

export interface CapitalContribution {
  id: number;
  journalNo: string | null;
  status: 'draft' | 'prepared' | 'posted' | 'void';
  postingDate: string;
  description: string;
  amount: number;
  fundingAccountCode: string;
  fundingAccountName: string;
  fundingMethod: 'bank_transfer' | 'cash';
  equityAccountCode: string;
  reference: string | null;
  preparerId: number | null;
  preparerName: string | null;
  approverName: string | null;
  createdAt: string;
}

export interface CapitalWorkspace {
  fundingAccounts: CapitalAccount[];
  equityAccounts: CapitalAccount[];
  branches: Array<{ id: number; code: string; name: string }>;
  contributions: CapitalContribution[];
}

const admin = (actor: PostingActor) => matchPerm(actor.permissions, 'admin:system:bypass::allow');

function amount(value: number) {
  if (!Number.isFinite(value) || value <= 0) throw new PostingError('Contribution amount must be greater than zero');
  return Math.round(value * 100) / 100;
}

export function canCreateCapital(actor: CapitalActor) {
  return actor.roleName === 'ceo' || admin(actor);
}

export function canVerifyCapital(actor: CapitalActor) {
  return admin(actor) || (
    (actor.departmentId === 'finance' || actor.departmentId === 'accounting')
    && matchPerm(actor.permissions, 'finance:journal:approve::allow')
  );
}

export async function loadCapitalWorkspace(): Promise<CapitalWorkspace> {
  const [funding, equity, branches, contributions] = await Promise.all([
    query<{ code: string; name: string; control_type: 'bank' | 'cash' }>(
      `SELECT code, name, control_type
         FROM finance.accounts
        WHERE active AND account_type = 'asset' AND control_type IN ('bank', 'cash')
        ORDER BY CASE control_type WHEN 'bank' THEN 1 ELSE 2 END, code`,
    ),
    query<{ code: string; name: string; control_type: null }>(
      `SELECT code, name, control_type
         FROM finance.accounts
        WHERE active AND account_type = 'equity'
        ORDER BY code`,
    ),
    query<{ id: string; code: string; name: string }>(
      `SELECT id::text, code, name FROM finance.branches WHERE active ORDER BY code`,
    ),
    query<{
      id: string;
      journal_no: string | null;
      status: CapitalContribution['status'];
      posting_date: string;
      description: string;
      amount: string;
      funding_account_code: string;
      funding_account_name: string;
      funding_method: CapitalContribution['fundingMethod'];
      equity_account_code: string;
      reference: string | null;
      preparer_id: number | null;
      preparer_name: string | null;
      approver_name: string | null;
      created_at: string;
    }>(
      `SELECT j.id::text, j.journal_no, j.status, j.posting_date::text, j.description,
              coalesce(sum(l.debit_thb) FILTER (WHERE a.control_type IN ('bank','cash')), 0)::text AS amount,
              coalesce(max(l.account_code) FILTER (WHERE a.control_type IN ('bank','cash')), '') AS funding_account_code,
              coalesce(max(a.name) FILTER (WHERE a.control_type IN ('bank','cash')), '') AS funding_account_name,
              coalesce(j.metadata->>'fundingMethod', 'bank_transfer') AS funding_method,
              coalesce(max(l.account_code) FILTER (WHERE a.account_type = 'equity'), '') AS equity_account_code,
              nullif(j.metadata->>'reference', '') AS reference,
              j.preparer_id, p.fullname AS preparer_name, v.fullname AS approver_name,
              j.created_at::text
         FROM finance.journals j
         JOIN finance.journal_lines l ON l.journal_id = j.id
         JOIN finance.accounts a ON a.code = l.account_code
         LEFT JOIN folio.users p ON p.id = j.preparer_id
         LEFT JOIN folio.users v ON v.id = j.approver_id
        WHERE j.source_type = 'capital_contribution'
        GROUP BY j.id, p.fullname, v.fullname
        ORDER BY j.created_at DESC
        LIMIT 100`,
    ),
  ]);
  return {
    fundingAccounts: funding.rows.map((row) => ({ code: row.code, name: row.name, controlType: row.control_type })),
    equityAccounts: equity.rows.map((row) => ({ code: row.code, name: row.name, controlType: row.control_type })),
    branches: branches.rows.map((row) => ({ id: Number(row.id), code: row.code, name: row.name })),
    contributions: contributions.rows.map((row) => ({
      id: Number(row.id),
      journalNo: row.journal_no,
      status: row.status,
      postingDate: row.posting_date,
      description: row.description,
      amount: Number(row.amount),
      fundingAccountCode: row.funding_account_code,
      fundingAccountName: row.funding_account_name,
      fundingMethod: row.funding_method,
      equityAccountCode: row.equity_account_code,
      reference: row.reference,
      preparerId: row.preparer_id,
      preparerName: row.preparer_name,
      approverName: row.approver_name,
      createdAt: row.created_at,
    })),
  };
}

export async function submitCapitalContribution(args: {
  actor: CapitalActor;
  postingDate: string;
  branchId: number;
  fundingAccountCode: string;
  equityAccountCode: string;
  reference: string;
  note: string;
  amount: number;
  requestKey: string;
}) {
  if (!canCreateCapital(args.actor)) throw new PostingError('Only the CEO can record a capital contribution');
  if (!/^\d{4}-\d{2}-\d{2}$/.test(args.postingDate)) throw new PostingError('Posting date must be YYYY-MM-DD');
  if (!/^[a-zA-Z0-9_-]{8,100}$/.test(args.requestKey)) throw new PostingError('A valid request key is required');
  const value = amount(args.amount);
  const [accounts, branch] = await Promise.all([
    query<{ code: string; name: string; account_type: string; control_type: string | null }>(
      `SELECT code, name, account_type, control_type
         FROM finance.accounts
        WHERE active AND code = ANY($1::text[])`,
      [[args.fundingAccountCode, args.equityAccountCode]],
    ),
    query<{ id: string }>(`SELECT id::text FROM finance.branches WHERE id = $1 AND active`, [args.branchId]),
  ]);
  if (!branch.rows[0]) throw new PostingError('An active branch is required');
  const funding = accounts.rows.find((row) => row.code === args.fundingAccountCode);
  const equity = accounts.rows.find((row) => row.code === args.equityAccountCode);
  if (!funding || funding.account_type !== 'asset' || !['bank', 'cash'].includes(funding.control_type ?? '')) {
    throw new PostingError('Funding must be deposited to an active bank or cash account');
  }
  if (!equity || equity.account_type !== 'equity') throw new PostingError('An active equity account is required');
  const method = funding.control_type === 'cash' ? 'cash' : 'bank_transfer';
  if (method === 'bank_transfer' && !args.reference.trim()) throw new PostingError('A bank transfer reference is required');
  const description = `Capital contribution · ${args.actor.fullname} · ${method === 'cash' ? 'cash' : 'bank transfer'}`;
  return preparePosting({
    postingDate: args.postingDate,
    description,
    sourceType: 'capital_contribution',
    sourceId: args.requestKey,
    sourceEventKey: `capital:${args.requestKey}`,
    branchId: args.branchId,
    metadata: {
      fundingMethod: method,
      fundingAccountCode: funding.code,
      equityAccountCode: equity.code,
      reference: args.reference.trim(),
      note: args.note.trim(),
      contributorId: args.actor.id,
      contributorName: args.actor.fullname,
    },
    lines: [
      { accountCode: funding.code, description: `${funding.name} funded by ${args.actor.fullname}`, debitThb: value, branchId: args.branchId },
      { accountCode: equity.code, description: `${equity.name} contributed by ${args.actor.fullname}`, creditThb: value, branchId: args.branchId },
    ],
  }, args.actor);
}

export async function verifyCapitalContribution(journalId: number, actor: CapitalActor) {
  if (!canVerifyCapital(actor)) throw new PostingError('Finance or Accounting approval is required');
  const found = await query<{ source_type: string; status: string; preparer_id: number | null }>(
    `SELECT source_type, status, preparer_id FROM finance.journals WHERE id = $1`,
    [journalId],
  );
  const row = found.rows[0];
  if (!row || row.source_type !== 'capital_contribution') throw new PostingError('Capital contribution not found');
  if (row.preparer_id === actor.id) throw new PostingError('The contributor cannot verify their own funding');
  return approveAndPostJournal(journalId, actor);
}
