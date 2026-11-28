import { BillingConsole } from "@erpx/billing-ui/components/billing-console";
import { requireManageErpxContext } from "@erpx/identity-access-server/public/authorization";
import { PageContent } from "@erpx/platform-ui/components/foundation";
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
