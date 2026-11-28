import {ExpenseNewWork} from "@erpx/people-payroll-ui/components/expense-purchase-context";
import {ExpenseEnvironment} from "@erpx/people-payroll-ui/components/expense-environment";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { ExpenseEditor } from "@erpx/people-payroll-ui/components/expense-controls";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import {expenseOptions} from '@erpx/people-payroll-server/public/expenses';

export const dynamic = "force-dynamic";

export default async function NewExpense({
  params,
  searchParams,
}: { params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const query=await searchParams,returnTo=typeof query.return==="string"?query.return:"view=tracking&tracking=requests";

  return (
    <ExpenseEnvironment actor={actor}><FeaturePage className="ui-page-content request-page" data-source="live">
      <Link href={canonicalUiHref(`${base}/work?${new URLSearchParams(returnTo)}`)}>← My work</Link>
      <PageHeader appearance="standard" eyebrow="Personal requests" title="Expense claim" description="Attach evidence, describe the expense, and review who receives it before sending." />
      <ExpenseNewWork><ExpenseEditor base={base} options={await expenseOptions(actor)} initialOpen returnTo={returnTo} /></ExpenseNewWork>
    </FeaturePage></ExpenseEnvironment>
  );
}
