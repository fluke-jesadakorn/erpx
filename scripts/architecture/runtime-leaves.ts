import type { WorkspaceGraph } from '../testing/workspace-graph';

const leaves = [
  'modules/commerce/server/src/public/commerce-order-read.ts',
  'modules/commerce/server/src/public/commerce-pages.ts',
  'modules/finance/server/src/public/customer-receipt-issue.ts',
  'modules/pos/server/src/public/stripe-provider.ts',
  'modules/identity-access/server/src/public/requested-holding-scope.ts',
  'modules/legal/server/src/public/attachment-read-parent.ts',
  'modules/people-payroll/server/src/public/expense-read.ts',
  'modules/people-payroll/server/src/public/expense-payment-read.ts',
  'modules/people-payroll/server/src/public/leave-read.ts',
  'modules/records/server/src/public/employee-requests-read.ts',
  'modules/collaboration/server/src/public/messaging-read.ts',
  'modules/collaboration/server/src/public/social-home-read.ts',

  'modules/operations/server/src/public/staff-task-read.ts',
  'modules/support/server/src/public/support-ticket-read.ts',
  'modules/support/server/src/public/support-evidence-planning.ts',
  'modules/workflow/server/src/public/flow-reader.ts',
  'modules/finance/server/src/public/money-primitives.ts',
  'modules/finance/server/src/public/company-money-rates.ts',
  'modules/finance/server/src/public/journal-planning.ts',
  'modules/people-payroll/server/src/public/people-evaluation-read.ts',
  'modules/collaboration/server/src/public/social-source-policy.ts',
  'modules/collaboration/server/src/public/work-social-read-policy.ts',
  'modules/learning/server/src/public/evaluation-source-policy.ts',
  'modules/projects/server/src/public/evaluation-source-policy.ts',
  'modules/operations/server/src/public/evaluation-source-policy.ts',
  'modules/support/server/src/public/support-incident-read.ts',
  'modules/support/server/src/adapters/read-runtime.ts',
  'modules/people-payroll/server/src/public/overtime-read.ts',
  'modules/finance/server/src/public/invoice-line-read.ts',
  'modules/finance/server/src/public/finance-invoice-read.ts',
  'modules/workflow/server/src/public/flow-record.ts',
  'modules/people-payroll/server/src/public/retirement-clearance-policy.ts',
  'modules/support/server/src/public/support-notification-sources.ts',
  'platform/runtime/server/src/public/runtime/sql.ts',
  'modules/workflow/server/src/public/flow-command-planning.ts',
  'modules/workflow/server/src/public/approval-planning.ts',
  'modules/operations/server/src/public/staff-policy.ts',
  'modules/support/server/src/public/service-team-reads.ts',
  'modules/records/server/src/public/request-override-read.ts',
  'modules/people-payroll/server/src/public/spending-template-read.ts',
  'modules/people-payroll/server/src/public/expense-company-history-scope.ts',
  'modules/organization/server/src/public/department-authority-reads.ts',
  'modules/organization/server/src/public/organization-roster-read.ts',
  'modules/organization/server/src/public/organization-graph-guard.ts',
  'modules/workflow/server/src/public/command-assertions.ts',
  'modules/workflow/server/src/public/command-guards.ts',
  'modules/identity-access/server/src/public/iam-service-reads.ts',
  'modules/identity-access/server/src/public/iam-grants.ts',
  'modules/identity-access/server/src/public/directory-lookup.ts',
  'modules/identity-access/server/src/public/directory-synchronization.ts',
  'platform/runtime/server/src/adapters/runtime/filesystem.ts',
  'platform/runtime/server/src/adapters/runtime/minio.ts',
  'modules/pms/server/src/public/feature-availability.ts',
  'modules/platform-administration/server/src/public/release-predicate.ts',
];
const commandCompositions = new Set([
  'modules/identity-access/server/src/adapters/iam-service.ts',
  'modules/identity-access/server/src/adapters/iam-directories.ts',
  'modules/workflow/server/src/adapters/commands.ts',
  'modules/legal/server/src/adapters/retention-execution.ts',
  'modules/identity-access/server/src/adapters/authorization.ts',
]);
/** These read/primitive entry points must not regain the business command graph. */
export function runtimeLeafViolations(graph: Pick<WorkspaceGraph, 'runtimeImports'>) {
  const violations: { source: string; target: string }[] = [];
  for (const source of leaves) {
    const pending = [source], visited = new Set<string>();
    for (let index = 0; index < pending.length; index++) {
      const file = pending[index];
      if (visited.has(file)) continue;
      visited.add(file);
      if (commandCompositions.has(file)) violations.push({ source, target: file });
      for (const dependency of graph.runtimeImports.get(file) ?? []) pending.push(dependency);
    }
  }
  return violations;
}
