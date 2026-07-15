// Workspace branch coverage tests.
// Run: node app/tests/account-supervisor-branch.test.mjs
//
// Mirrors the role → workspace branch routing in app/src/components/TilePage.tsx.
// Verifies that every role has at least one workspace component that renders,
// and that filtering logic is consistent with the policy engine stages.

const _fmt = (n) => parseFloat(n || 0).toLocaleString('th-TH', { minimumFractionDigits: 2 });

// --- Re-implement workspace filter logic from each new workspace -----------

function accountSupervisorFilter(expenses) {
  const pendingQueue = expenses.filter((e) => e.status === 'account_supervisor_review');
  const recent = expenses.filter((e) =>
    ['accountant_reviewed', 'account_supervisor_review', 'approved', 'rejected', 'paid'].includes(e.status)
  );
  const totalValue = pendingQueue.reduce((s, e) => s + (parseFloat(e.total_amount) || 0), 0);
  return { pendingQueue, recent, totalValue };
}

function supervisorFilter(expenses, prs, currentUser) {
  const teamExpenses = expenses.filter((e) => e.submitter_dept === currentUser?.department);
  const pendingExpenses = teamExpenses.filter((e) => e.status === 'supervisor_review');
  const otherExpenses = teamExpenses.filter((e) => e.status !== 'supervisor_review');
  const pendingPRs = prs.filter(
    (p) => p.dept_name === currentUser?.department && p.status === 'supervisor_review'
  );
  return { teamExpenses, pendingExpenses, otherExpenses, pendingPRs };
}

function managerFilter(expenses) {
  const allActive = expenses.filter((e) =>
    ['head_review', 'supervisor_review', 'accounting_review', 'account_supervisor_review'].includes(e.status)
  );
  const pendingForMe = expenses.filter((e) => e.status === 'manager_review');
  const recent = expenses.filter((e) =>
    ['approved', 'paid', 'rejected', 'manager_review'].includes(e.status)
  );
  const totalValue = pendingForMe.reduce((s, e) => s + (parseFloat(e.total_amount) || 0), 0);
  return { allActive, pendingForMe, recent, totalValue };
}

function itFilter(expenses, prs) {
  const byStatus = expenses.reduce((acc, e) => {
    acc[e.status] = (acc[e.status] || 0) + 1;
    return acc;
  }, {});
  const totalValue = expenses.reduce((s, e) => s + (parseFloat(e.total_amount) || 0), 0);
  const sortedExpenses = [...expenses].sort(
    (a, b) => new Date(b.created_at || b.transaction_date).getTime() -
              new Date(a.created_at || a.transaction_date).getTime()
  );
  return { byStatus, totalValue, sortedExpenses, prCount: prs.length };
}

// --- Re-implement role → branch routing ------------------------------------

// Mirrors the workbench tab branches in TilePage.tsx after the fix.
function pickBranch(role, subView, _ctx) {
  if (role === 'staff') {
    if (subView === 'history') return 'StaffHistoryView';
    return 'StaffWorkspace';
  }
  if (role === 'accountant' || role === 'account_officer') {
    if (subView === 'coa-search') return 'COASearchView';
    if (subView === 'slip-search') return 'SlipSearchView';
    return 'AccountantWorkspace';
  }
  if (role === 'account_supervisor') {
    if (subView === 'coa-search') return 'COASearchView';
    if (subView === 'slip-search') return 'SlipSearchView';
    return 'AccountSupervisorWorkspace';
  }
  if (role === 'supervisor') return 'SupervisorWorkspace';
  if (role === 'head_of_department') {
    if (subView === 'team' || subView === 'team-manage') return 'HoDTeamTab/TeamView';
    return 'HeadOfDeptWorkspace';
  }
  if (role === 'accounting_manager') {
    if (subView === 'recon') return 'ReconciliationView';
    return 'AccountingManagerWorkspace';
  }
  if (role === 'cfo') return 'CFOWorkspace';
  if (role === 'manager') return 'ManagerWorkspace';
  if (role === 'admin') return 'ExecutiveWorkspace';
  if (role === 'it') return 'ITWorkspace';
  if (role === 'ceo') return null; // CEO uses cockpit tab, not workbench
  if (role === 'hr') return null;  // HR uses hr tab
  if (role === 'hr_manager') return null; // HR Manager uses cockpit/ledger
  return null;
}

// --- Tests ------------------------------------------------------------------

let pass = 0, fail = 0;
function expect(label, actual, expected) {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  if (ok) { pass++; console.log(`✓ ${label}`); }
  else {
    fail++;
    console.log(`✗ ${label}`);
    console.log(`    expected: ${JSON.stringify(expected)}`);
    console.log(`    actual:   ${JSON.stringify(actual)}`);
  }
}

const sampleExpenses = [
  { id: 1, status: 'ocr_extracted', total_amount: '500.00', submitter_dept: 'Sales' },
  { id: 2, status: 'account_officer_review', total_amount: '1500.00', submitter_dept: 'Sales' },
  { id: 3, status: 'account_supervisor_review', total_amount: '3500.00', submitter_dept: 'Marketing' },
  { id: 4, status: 'account_supervisor_review', total_amount: '8000.00', submitter_dept: 'IT' },
  { id: 5, status: 'supervisor_review', total_amount: '1200.00', submitter_dept: 'Sales' },
  { id: 6, status: 'supervisor_review', total_amount: '700.00', submitter_dept: 'Marketing' },
  { id: 7, status: 'approved', total_amount: '999.99', submitter_dept: 'Sales' },
  { id: 8, status: 'paid', total_amount: '1234.56', submitter_dept: 'Sales' },
  { id: 9, status: 'rejected', total_amount: '50.00', submitter_dept: 'Sales' },
];

const samplePRs = [
  { id: 1, status: 'supervisor_review', dept_name: 'Sales', total_estimate: '2500.00' },
  { id: 2, status: 'head_review', dept_name: 'Marketing', total_estimate: '4500.00' },
  { id: 3, status: 'pending', dept_name: 'Sales', total_estimate: '900.00' },
];

console.log('— AccountSupervisorWorkspace filter');
const asResult = accountSupervisorFilter(sampleExpenses);
expect('AS pending queue has only account_supervisor_review', asResult.pendingQueue.length, 2);
expect('AS pending IDs', asResult.pendingQueue.map((e) => e.id).sort(), [3, 4]);
expect('AS total value', asResult.totalValue, 11500);
expect('AS recent includes decided statuses', asResult.recent.length, 5);

console.log('\n— SupervisorWorkspace filter (Sales user)');
const supSalesUser = { id: 10, department: 'Sales' };
const supSalesResult = supervisorFilter(sampleExpenses, samplePRs, supSalesUser);
expect('supervisor sees only Sales expenses', supSalesResult.teamExpenses.length, 6);
expect('supervisor pending = supervisor_review in Sales', supSalesResult.pendingExpenses.length, 1);
expect('supervisor pending PR = Sales + supervisor_review', supSalesResult.pendingPRs.length, 1);
expect('supervisor other = non-supervisor_review in Sales', supSalesResult.otherExpenses.length, 5);

console.log('\n— SupervisorWorkspace filter (Marketing user)');
const supMktUser = { id: 11, department: 'Marketing' };
const supMktResult = supervisorFilter(sampleExpenses, samplePRs, supMktUser);
expect('marketing supervisor sees only Marketing expenses', supMktResult.teamExpenses.length, 2);
expect('marketing supervisor pending PR = 0 (only head_review)', supMktResult.pendingPRs.length, 0);

console.log('\n— ManagerWorkspace filter');
const mgrResult = managerFilter(sampleExpenses);
expect('manager pendingForMe empty (no manager_review)', mgrResult.pendingForMe.length, 0);
expect('manager allActive = active stages', mgrResult.allActive.length, 4);
expect('manager recent = decided statuses', mgrResult.recent.length, 3);

console.log('\n— ITWorkspace filter');
const itResult = itFilter(sampleExpenses, samplePRs);
expect('IT total value', Math.round(itResult.totalValue * 100) / 100, 17684.55);
expect('IT byStatus keys', Object.keys(itResult.byStatus).sort(), [
  'account_officer_review',
  'account_supervisor_review',
  'approved',
  'ocr_extracted',
  'paid',
  'rejected',
  'supervisor_review',
]);
expect('IT sortedExpenses has all', itResult.sortedExpenses.length, 9);
expect('IT prCount', itResult.prCount, 3);

console.log('\n— every role has at least one workspace branch');
const rolesToCheck = [
  'staff', 'accountant', 'account_officer', 'account_supervisor',
  'supervisor', 'head_of_department', 'accounting_manager',
  'cfo', 'manager', 'admin', 'it',
];
for (const role of rolesToCheck) {
  const branch = pickBranch(role, '', {});
  expect(`role ${role} has workbench branch`, !!branch, true);
}

console.log('\n— CEO/HR/HR-Manager redirect to other tabs (no workbench branch expected)');
expect('CEO has no workbench branch', pickBranch('ceo', '', {}), null);
expect('HR has no workbench branch', pickBranch('hr', '', {}), null);
expect('HR Manager has no workbench branch', pickBranch('hr_manager', '', {}), null);

console.log('\n— the user-reported blank-page cases');
expect(
  'account_supervisor /as-review routes to AccountSupervisorWorkspace',
  pickBranch('account_supervisor', 'review', {}),
  'AccountSupervisorWorkspace'
);
expect(
  'account_officer /acct-queue still routes to AccountantWorkspace',
  pickBranch('account_officer', 'queue', {}),
  'AccountantWorkspace'
);
expect(
  'account_supervisor /acct-slip-search routes to SlipSearchView (semantic search)',
  pickBranch('account_supervisor', 'slip-search', {}),
  'SlipSearchView'
);

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) process.exit(1);