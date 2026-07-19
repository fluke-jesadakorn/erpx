import 'server-only';
import { query } from '@/db';

export interface OpsStep {
  label: string;
  owner: string;
  count: number;
  amount: number;
}

export interface OpsFlow {
  id: 'capital' | 'expense' | 'sales';
  title: string;
  subtitle: string;
  href: string;
  openCount: number;
  openAmount: number;
  steps: OpsStep[];
}

export interface AccountingOps {
  cashBalance: number;
  postedThisMonth: number;
  postedJournalCount: number;
  pendingJournalCount: number;
  flows: OpsFlow[];
  recent: Array<{
    id: number;
    journalNo: string;
    postingDate: string;
    description: string;
    sourceType: string;
    amount: number;
  }>;
}

interface WaybillCount {
  origin: 'expense' | 'so';
  current_stage: string;
  status: string;
  count: string;
  amount: string;
}

interface JournalCount {
  source_type: string;
  status: string;
  count: string;
  amount: string;
}

function bucket(rows: WaybillCount[], origin: WaybillCount['origin'], stages: string[]) {
  const selected = rows.filter((row) => row.origin === origin && stages.includes(row.current_stage));
  return {
    count: selected.reduce((sum, row) => sum + Number(row.count), 0),
    amount: selected.reduce((sum, row) => sum + Number(row.amount), 0),
  };
}

export async function loadAccountingOps(): Promise<AccountingOps> {
  const [waybills, journals, metrics, recent] = await Promise.all([
    query<WaybillCount>(
      `SELECT origin, current_stage, status, count(*)::text,
              coalesce(sum(total_amount), 0)::text AS amount
         FROM folio.waybills
        WHERE origin IN ('expense','so')
        GROUP BY origin, current_stage, status`,
    ),
    query<JournalCount>(
      `SELECT j.source_type, j.status, count(DISTINCT j.id)::text,
              coalesce(sum(l.debit_thb), 0)::text AS amount
         FROM finance.journals j
         JOIN finance.journal_lines l ON l.journal_id = j.id
        GROUP BY j.source_type, j.status`,
    ),
    query<{ cash_balance: string; posted_month: string; posted_count: string; pending_count: string }>(
      `SELECT
         coalesce(sum(l.debit_thb - l.credit_thb) FILTER (
           WHERE j.status = 'posted' AND a.control_type IN ('bank','cash')
         ), 0)::text AS cash_balance,
         coalesce(sum(l.debit_thb) FILTER (
           WHERE j.status = 'posted' AND j.posting_date >= date_trunc('month', current_date)
         ), 0)::text AS posted_month,
         count(DISTINCT j.id) FILTER (
           WHERE j.status = 'posted' AND j.posting_date >= date_trunc('month', current_date)
         )::text AS posted_count,
         count(DISTINCT j.id) FILTER (WHERE j.status IN ('draft','prepared'))::text AS pending_count
       FROM finance.journals j
       JOIN finance.journal_lines l ON l.journal_id = j.id
       JOIN finance.accounts a ON a.code = l.account_code`,
    ),
    query<{ id: string; journal_no: string; posting_date: string; description: string; source_type: string; amount: string }>(
      `SELECT j.id::text, coalesce(j.journal_no, 'Draft #' || j.id::text) AS journal_no,
              j.posting_date::text, j.description, j.source_type,
              sum(l.debit_thb)::text AS amount
         FROM finance.journals j
         JOIN finance.journal_lines l ON l.journal_id = j.id
        WHERE j.status = 'posted'
        GROUP BY j.id
        ORDER BY j.posting_date DESC, j.id DESC
        LIMIT 8`,
    ),
  ]);
  const wb = waybills.rows;
  const active = (origin: WaybillCount['origin']) => wb.filter((row) => row.origin === origin && row.status === 'open');
  const total = (rows: Array<{ count: string; amount: string }>) => ({
    count: rows.reduce((sum, row) => sum + Number(row.count), 0),
    amount: rows.reduce((sum, row) => sum + Number(row.amount), 0),
  });
  const expense = total(active('expense'));
  const sales = total(active('so'));
  const capital = journals.rows.filter((row) => row.source_type === 'capital_contribution');
  const capitalAt = (status: string) => total(capital.filter((row) => row.status === status).map((row) => ({ ...row, origin: 'expense', current_stage: status })));
  const capitalAll = total(capital.map((row) => ({ ...row, origin: 'expense', current_stage: row.status })));
  const metric = metrics.rows[0];
  return {
    cashBalance: Number(metric?.cash_balance ?? 0),
    postedThisMonth: Number(metric?.posted_month ?? 0),
    postedJournalCount: Number(metric?.posted_count ?? 0),
    pendingJournalCount: Number(metric?.pending_count ?? 0),
    flows: [
      {
        id: 'capital',
        title: 'Capital funding',
        subtitle: 'CEO records funding; Finance independently verifies the GL entry.',
        href: '/capital',
        openCount: capitalAt('prepared').count,
        openAmount: capitalAt('prepared').amount,
        steps: [
          { label: 'CEO records cash or transfer', owner: 'CEO', ...capitalAll },
          { label: 'Verify source and amount', owner: 'Finance / Accounting', ...capitalAt('prepared') },
          { label: 'Post bank or cash ↔ equity', owner: 'General ledger', ...capitalAt('posted') },
        ],
      },
      {
        id: 'expense',
        title: 'Expenses',
        subtitle: 'Claims become an accrual, a controlled payment, then a settled payable.',
        href: '/expense?scope=all',
        openCount: expense.count,
        openAmount: expense.amount,
        steps: [
          { label: 'Claim and department approval', owner: 'Employee / Department', ...bucket(wb, 'expense', ['submission','department_approval']) },
          { label: 'Review and accrue expense', owner: 'Accounting', ...bucket(wb, 'expense', ['accounting_review','accounting_approval','executive_approval']) },
          { label: 'Pay and settle payable', owner: 'Finance / Accounting', ...bucket(wb, 'expense', ['payment','settlement']) },
          { label: 'Complete', owner: 'General ledger', ...bucket(wb, 'expense', ['completed']) },
        ],
      },
      {
        id: 'sales',
        title: 'Sales orders',
        subtitle: 'An approved order becomes revenue and AR, then cash clears the receivable.',
        href: '/sales?scope=all',
        openCount: sales.count,
        openAmount: sales.amount,
        steps: [
          { label: 'Order review and approval', owner: 'Sales', ...bucket(wb, 'so', ['so_draft','so_sales_review','so_dept_approval']) },
          { label: 'Credit check and invoice', owner: 'Accounting', ...bucket(wb, 'so', ['so_credit_check']) },
          { label: 'Collect and settle AR', owner: 'Finance', ...bucket(wb, 'so', ['so_invoiced']) },
          { label: 'Complete', owner: 'General ledger', ...bucket(wb, 'so', ['so_paid','completed']) },
        ],
      },
    ],
    recent: recent.rows.map((row) => ({
      id: Number(row.id),
      journalNo: row.journal_no,
      postingDate: row.posting_date,
      description: row.description,
      sourceType: row.source_type,
      amount: Number(row.amount),
    })),
  };
}
