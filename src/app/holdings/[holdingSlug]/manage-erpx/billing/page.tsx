import { BillingConsole } from "@/components/billing/billing-console";
import { requireManageErpxContext } from "@/server/authorization";
export default async function BillingPage() {
  const { platform: a } = await requireManageErpxContext();
  if (!["platform_admin", "platform_auditor"].includes(a.role))
    throw new Error("FORBIDDEN");
  return (
    <div className="content-stack">
      <BillingConsole platform writable={a.role === "platform_admin"} />
    </div>
  );
}
