import { BillingConsole } from "@/components/billing/billing-console";
import { requireManageErpxContext } from "@/server/authorization";
import { PageContent } from "@/components/ui/foundation";
export default async function BillingPage() {
  const { platform: a } = await requireManageErpxContext();
  if (!["platform_admin", "platform_auditor"].includes(a.role))
    throw new Error("FORBIDDEN");
  return (
    <PageContent as="div" className="ui-page-content">
      <BillingConsole platform writable={a.role === "platform_admin"} />
    </PageContent>
  );
}
