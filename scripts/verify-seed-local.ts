import { getLocalProxy } from "./local/fixtures";


const platform = await getLocalProxy({ configPath: "wrangler.jsonc", persist: true, remoteBindings: false });
const number = (value: unknown) => Number(value ?? 0);
const checksum = async (bytes: ArrayBuffer) => Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), (byte) => byte.toString(16).padStart(2, "0")).join("");

try {
  const report: Record<string, unknown> = {};
  const users = await platform.env.CONTROL_DB.prepare("SELECT COUNT(*) AS count FROM \"user\"").first<{ count: number }>();
  report.users = number(users?.count);

  for (const [id, db, files] of [["h001", platform.env.H001_DB, platform.env.H001_FILES], ["h002", platform.env.H002_DB, platform.env.H002_FILES]] as const) {
    const count = async (table: string, where = "1=1", args: unknown[] = []) => number((await db.prepare(`SELECT COUNT(*) AS count FROM ${table} WHERE ${where}`).bind(...args).first<{ count: number }>())?.count);
    const foreignKeys = await db.prepare("PRAGMA foreign_key_check").all();
    if (foreignKeys.results.length) throw new Error(`${id}: foreign key check failed (${foreignKeys.results.length} rows)`);

    const badJournals = await db.prepare("SELECT j.id FROM finance_journals j WHERE j.status IN ('posted','reversed') AND (SELECT COALESCE(SUM(base_debit_minor),0) FROM finance_journal_lines l WHERE l.journal_id=j.id) != (SELECT COALESCE(SUM(base_credit_minor),0) FROM finance_journal_lines l WHERE l.journal_id=j.id)").all<{ id: string }>();
    if (badJournals.results.length) throw new Error(`${id}: unbalanced posted journals (${badJournals.results.map((row) => row.id).join(", ")})`);

    const payrollRuns = await db.prepare("SELECT id,totals_json FROM hr_payroll_runs WHERE id LIKE 'payrun-%'").all<{ id: string; totals_json: string }>();
    for (const run of payrollRuns.results) {
      const items = await db.prepare("SELECT gross_minor,deductions_minor,employer_cost_minor,net_minor FROM hr_payroll_items WHERE run_id=?").bind(run.id).all<{ gross_minor: number; deductions_minor: number; employer_cost_minor: number; net_minor: number }>();
      const sums = items.results.reduce((total, item) => ({
        gross: total.gross + number(item.gross_minor),
        deductions: total.deductions + number(item.deductions_minor),
        employer: total.employer + number(item.employer_cost_minor),
        net: total.net + number(item.net_minor),
      }), { gross: 0, deductions: 0, employer: 0, net: 0 });
      const declared = JSON.parse(run.totals_json) as { gross?: number; deductions?: number; employer?: number; net?: number; headcount?: number };
      if (declared.gross !== sums.gross || declared.deductions !== sums.deductions || declared.employer !== sums.employer || declared.net !== sums.net || declared.headcount !== items.results.length)
        throw new Error(`${id}: payroll totals mismatch for ${run.id}`);
    }

    const allocations = await db.prepare(`SELECT p.id,p.amount_minor,p.outstanding_minor,COALESCE(SUM(a.amount_minor),0) AS allocated
      FROM finance_payments p LEFT JOIN finance_allocations a ON a.payment_id=p.id
      WHERE p.id LIKE 'payment-%' GROUP BY p.id`).all<{ id: string; amount_minor: number; outstanding_minor: number; allocated: number }>();
    for (const payment of allocations.results) {
      const allocated = number(payment.allocated);
      if (allocated > number(payment.amount_minor) || number(payment.amount_minor) - number(payment.outstanding_minor) !== allocated)
        throw new Error(`${id}: payment allocation mismatch for ${payment.id}`);
    }
    const invoiceAllocations = await db.prepare(`SELECT i.id,i.total_minor,COALESCE(SUM(a.amount_minor),0) AS allocated
      FROM finance_invoices i LEFT JOIN finance_allocations a ON a.invoice_id=i.id
      WHERE i.id LIKE 'invoice-%' GROUP BY i.id`).all<{ id: string; total_minor: number; allocated: number }>();
    if (invoiceAllocations.results.some((invoice) => number(invoice.allocated) > number(invoice.total_minor))) throw new Error(`${id}: invoice allocation exceeds total`);

    const stockBalances=await db.prepare(`SELECT s.product_id,s.warehouse_id FROM commerce_stock s LEFT JOIN (SELECT warehouse_id,product_id,SUM(quantity_milli) qty,SUM(value_minor) value FROM inventory_ledger GROUP BY warehouse_id,product_id) m ON m.product_id=s.product_id AND m.warehouse_id=s.warehouse_id WHERE s.quantity_milli!=COALESCE(m.qty,0) OR s.value_minor!=COALESCE(m.value,0)
      UNION ALL SELECT m.product_id,m.warehouse_id FROM inventory_ledger m LEFT JOIN commerce_stock s ON s.product_id=m.product_id AND s.warehouse_id=m.warehouse_id WHERE s.product_id IS NULL`).all();
    if(stockBalances.results.length)throw new Error(`${id}: stock does not match inventory movement ledger`);
    const transferPairs=await db.prepare(`SELECT t.id FROM inventory_transfers t WHERE t.status='posted' AND (t.approved_by IS NULL OR t.approved_by IN (t.created_by,t.prepared_by) OR t.total_value_minor!=(SELECT COALESCE(SUM(value_minor),0) FROM inventory_transfer_lines WHERE transfer_id=t.id)
      OR EXISTS(SELECT 1 FROM inventory_transfer_lines l WHERE l.transfer_id=t.id AND (NOT EXISTS(SELECT 1 FROM inventory_transfer_moves m WHERE m.transfer_id=t.id AND m.product_id=l.product_id AND m.warehouse_id=t.source_warehouse_id AND m.quantity_milli=-l.quantity_milli AND m.value_minor=-l.value_minor) OR NOT EXISTS(SELECT 1 FROM inventory_transfer_moves m WHERE m.transfer_id=t.id AND m.product_id=l.product_id AND m.warehouse_id=t.target_warehouse_id AND m.quantity_milli=l.quantity_milli AND m.value_minor=l.value_minor))))
      UNION ALL SELECT m.transfer_id FROM inventory_transfer_moves m JOIN inventory_transfers t ON t.id=m.transfer_id WHERE t.status!='posted'
      UNION ALL SELECT transfer_id FROM inventory_transfer_moves GROUP BY transfer_id,product_id HAVING COUNT(*)!=2 OR SUM(quantity_milli)!=0 OR SUM(value_minor)!=0`).all();
    if(transferPairs.results.length)throw new Error(`${id}: warehouse transfer movement or approval mismatch`);
    const countPostings=await db.prepare(`SELECT c.id FROM inventory_counts c WHERE c.status='posted' AND (c.approved_by IS NULL OR c.approved_by IN (c.created_by,c.prepared_by) OR (c.journal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=c.journal_id AND j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted')) OR c.net_value_minor!=(SELECT COALESCE(SUM(value_minor),0) FROM inventory_adjustment_moves WHERE count_id=c.id))`).all();
    if(countPostings.results.length)throw new Error(`${id}: stock count posting or independent approval mismatch`);

    const expenseBalances=await db.prepare(`SELECT x.id FROM finance_expenses x LEFT JOIN finance_invoices i ON i.id=x.invoice_id LEFT JOIN finance_parties p ON p.id=i.party_id
      WHERE x.invoice_id IS NOT NULL AND (i.id IS NULL OR i.company_id!=x.company_id OR i.source_type!='expense_claim' OR i.source_id!=x.id OR p.kind!='employee' OR p.person_id!=x.employee_id OR i.currency!=x.currency OR i.total_minor!=x.amount_minor+x.tax_minor
      OR (x.cancelled_at IS NULL AND i.outstanding_minor!=i.total_minor-(SELECT COALESCE(SUM(amount_minor),0) FROM finance_allocations a WHERE a.invoice_id=i.id))
      OR (x.status='reimbursed' AND (i.status!='paid' OR i.outstanding_minor!=0)) OR (x.status='posted' AND i.status NOT IN ('posted','partially_paid'))
      OR (x.cancelled_at IS NOT NULL AND (i.status!='voided' OR i.outstanding_minor!=0 OR x.reversal_journal_id IS NULL)))`).all<{id:string}>();
    if(expenseBalances.results.length)throw new Error(`${id}: expense/employee/payable mismatch: ${expenseBalances.results.map(r=>r.id).join(", ")}`);

    const statements = await db.prepare("SELECT id,opening_minor,closing_minor FROM finance_bank_statements WHERE id LIKE 'statement-%'").all<{ id: string; opening_minor: number; closing_minor: number }>();
    for (const statement of statements.results) {
      const rows = await db.prepare("SELECT amount_minor FROM finance_bank_transactions WHERE statement_id=?").bind(statement.id).all<{ amount_minor: number }>();
      const expected = number(statement.opening_minor) + rows.results.reduce((sum, row) => sum + number(row.amount_minor), 0);
      if (expected !== number(statement.closing_minor)) throw new Error(`${id}: bank statement arithmetic mismatch for ${statement.id}`);
    }

    const badMatches=await db.prepare("SELECT COUNT(*) count FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id LEFT JOIN finance_payments p ON p.id=t.matched_id LEFT JOIN finance_journals j ON j.id=p.journal_id WHERE t.match_status='matched' AND (t.matched_type!='payment' OR p.id IS NULL OR p.company_id!=t.company_id OR p.currency!=t.currency OR p.bank_account!=s.bank_account OR j.id IS NULL OR j.status!='posted' OR t.amount_minor!=CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -p.amount_minor END)").first<{count:number}>();
    if(number(badMatches?.count))throw new Error(`${id}: unsupported bank-payment match`);
    const selfLeaveReviews = await db.prepare("SELECT COUNT(*) AS count FROM leave_requests WHERE id LIKE 'seed-%' AND decided_by IS NOT NULL AND decided_by=created_by").first<{ count: number }>();
    const selfPayrollReviews = await db.prepare("SELECT COUNT(*) AS count FROM hr_payroll_runs WHERE id LIKE 'payrun-%' AND approved_by IS NOT NULL AND approved_by=created_by").first<{ count: number }>();
    const selfJournalReviews = await db.prepare("SELECT COUNT(*) AS count FROM finance_journals WHERE source_type IN ('manual','opening_import') AND posted_by IS NOT NULL AND posted_by=created_by").first<{ count: number }>();
    if (number(selfLeaveReviews?.count) || number(selfPayrollReviews?.count) || number(selfJournalReviews?.count)) throw new Error(`${id}: seeded approval actor was also the creator`);
    const auditEvents = await count("audit_events", "id LIKE 'audit-%'");
    if (auditEvents === 0) throw new Error(`${id}: seeded audit history is missing`);

    const objects = await files.list({ prefix: `${id}/` });
    const attachments = await db.prepare("SELECT object_key,bytes,checksum_sha256 FROM attachments WHERE id LIKE ?").bind(`attachment-${id}-%`).all<{ object_key: string; bytes: number; checksum_sha256: string }>();
    for (const attachment of attachments.results) {
      const object = await files.head(attachment.object_key);
      const body = await files.get(attachment.object_key);
      if (!object || !body) throw new Error(`${id} attachment is missing: ${attachment.object_key}`);
      const bytes = await body.arrayBuffer();
      if (object.size !== number(attachment.bytes) || bytes.byteLength !== number(attachment.bytes) || object.customMetadata?.sha256 !== attachment.checksum_sha256 || await checksum(bytes) !== attachment.checksum_sha256) throw new Error(`${id} attachment integrity mismatch: ${attachment.object_key}`);
      const leave = await db.prepare("SELECT number FROM leave_requests l JOIN attachments a ON a.owner_id=l.id WHERE a.object_key=?").bind(attachment.object_key).first<{ number: string }>();
      if (!leave?.number || !new TextDecoder().decode(bytes).includes(leave.number)) throw new Error(`${id} attachment does not reference its leave request: ${attachment.object_key}`);
    }
    const paymentFiles = await db.prepare("SELECT file_key,file_checksum,total_minor,id FROM hr_payment_batches WHERE id LIKE 'payment-batch-%' AND file_key IS NOT NULL").all<{ file_key: string; file_checksum: string | null; total_minor: number; id: string }>();
    for (const payment of paymentFiles.results) {
      const object = await files.head(payment.file_key);
      if (!object || (payment.file_checksum && object.customMetadata?.sha256 !== payment.file_checksum)) throw new Error(`${id} payment export integrity mismatch: ${payment.file_key}`);
      const body = await files.get(payment.file_key);
      if (!body) throw new Error(`${id} payment export is missing: ${payment.file_key}`);
      const bytes = await body.arrayBuffer();
      if (object.size !== bytes.byteLength || !payment.file_checksum || await checksum(bytes) !== payment.file_checksum) throw new Error(`${id} payment export checksum mismatch: ${payment.file_key}`);
      const csv = new TextDecoder().decode(bytes);
      const rows = csv.trim().split(/\r?\n/).slice(1);
      const items = await db.prepare("SELECT reference,payee_name,amount_minor,currency FROM hr_payment_batch_items WHERE batch_id=? ORDER BY reference").bind(payment.id).all<{ reference: string; payee_name: string; amount_minor: number; currency: string }>();
      const itemTotal = items.results.reduce((sum, row) => sum + number(row.amount_minor), 0);
      const csvRows = rows.map((row) => row.split(",")).sort((a, b) => a[0].localeCompare(b[0]));
      const csvTotal = csvRows.reduce((sum, row) => sum + Math.round(number(row[2]) * 100), 0);
      if (csvRows.some((row, index) => row[0] !== items.results[index]?.reference || row[1] !== items.results[index]?.payee_name || row[3] !== items.results[index]?.currency || Math.round(number(row[2]) * 100) !== number(items.results[index]?.amount_minor)))
        throw new Error(`${id} payment export row does not match ${payment.id}`);
      if (rows.length !== items.results.length || itemTotal !== number(payment.total_minor) || csvTotal !== itemTotal || object.size !== new TextEncoder().encode(csv).byteLength)
        throw new Error(`${id} payment export contents do not match ${payment.id}`);
    }

    report[id] = {
      companies: await count("organization_nodes", "kind='company'"),
      branches: await count("organization_nodes", "kind='branch'"),
      workers: await count("hr_people"),
      engagements: await count("hr_engagements"),
      leaveRequests: await count("leave_requests", "id LIKE ?", [`seed-${id}-leave-%`]),
      payrollRuns: await count("hr_payroll_runs"),
      journals: await count("finance_journals"),
      invoices: await count("finance_invoices"),
      payments: await count("finance_payments"),
      bankTransactions: await count("finance_bank_transactions"),
      posts: await count("social_posts", "id LIKE ?", [`seed-${id}-post-%`]),
      stories: await count("social_stories", "id LIKE ?", [`seed-${id}-story-%`]),
      attachments: attachments.results.length,
      paymentExports: paymentFiles.results.length,
      auditEvents,
      r2Objects: objects.objects.length,
      unbalancedPostedJournals: badJournals.results.length,
      foreignKeyErrors: foreignKeys.results.length,
    };
  }
  console.log(JSON.stringify(report, null, 2));
} finally {
  await platform.dispose();
}
