import {ExpenseNewWork} from "@/components/people_payroll/expense-purchase-context";
import {ExpenseEnvironment} from "@/components/people_payroll/expense-environment";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink as Link } from "@/components/records/record-link";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { ExpenseEditor } from "@/components/people_payroll/expense-controls";
import { PageHeader } from "@/components/ui/primitives";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { expenseOptions } from "@/server/modules/people_payroll/public/expenses";

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
