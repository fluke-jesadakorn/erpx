import { FinanceRecordWorkspace, FinanceLegacyHeading } from "@/components/finance/finance-record-workspace";
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
}: PageProps<"/holdings/[holdingSlug]/expenses/new">) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));

  return (
    <FeaturePage className="content-stack" data-source="live">
      <FinanceRecordWorkspace base={base} active="receivables" title="New expense claim" description="Capture what was paid or needs paying, retain evidence, and send it for approval."><FinanceLegacyHeading><Link className="back-link" href={`${base}/expenses`}>
        ← Expense claims
      </Link>
      <PageHeader
        eyebrow="Finance · Personal requests"
        title="Claim an expense or request a payment"
        description="Tell us what was paid or needs paying. Attach evidence, review the details, and send it for approval."
      />
      </FinanceLegacyHeading><ExpenseEditor base={base} options={await expenseOptions(actor)} initialOpen />
    </FinanceRecordWorkspace></FeaturePage>
  );
}
