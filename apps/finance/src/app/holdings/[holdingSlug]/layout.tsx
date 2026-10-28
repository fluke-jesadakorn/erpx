// App-owned entry; shared payloads do not import another application.
import WorkplaceLayout from "@erpx/workplace/routes/holdings/[holdingSlug]/layout";
export const dynamic = "force-dynamic";
export default function FinanceHoldingLayout(props: LayoutProps<"/holdings/[holdingSlug]">) {
  return WorkplaceLayout(props);
}
