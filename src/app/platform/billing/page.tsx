import { BillingConsole } from "@/components/billing/billing-console";
import { requirePlatformContext } from "@/server/authorization";
export default async function BillingPage() {
  const a = await requirePlatformContext();
  if (!["platform_admin", "platform_auditor"].includes(a.role))
    throw new Error("FORBIDDEN");
  return (
    <main className="content-stack">
      <BillingConsole platform writable={a.role === "platform_admin"} />
    </main>
  );
}
