import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { ExpenseEditor } from "@/components/modules/expense-controls";
import { PageHeader } from "@/components/ui/primitives";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { expenseOptions } from "@/server/expenses";

export const dynamic = "force-dynamic";

export default async function NewExpense({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/expenses/new">) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const query=await searchParams,returnTo=typeof query.return==="string"?query.return:"view=tracking&tracking=requests";

  return (
    <FeaturePage className="ui-page-content request-page" data-source="live">
      <Link href={`${base}/work?${new URLSearchParams(returnTo)}`}>← My work</Link>
      <PageHeader appearance="standard" eyebrow="Personal requests" title="Expense claim" description="Attach evidence, describe the expense, and review who receives it before sending." />
      <ExpenseEditor base={base} options={await expenseOptions(actor)} initialOpen returnTo={returnTo} />
    </FeaturePage>
  );
}
