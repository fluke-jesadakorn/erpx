import { BillingConsole } from "@erpx/feature-billing/components/billing-console";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import { PageContent } from "@erpx/ui/components/foundation";
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
