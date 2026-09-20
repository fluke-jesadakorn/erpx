import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { currentSession } from "@/server/authorization";
import Link from "next/link";
import { redirect } from "next/navigation";
import { AccountSecurity } from "./security-form";
export default async function SecurityPage() {
  try {
    await currentSession();
  } catch {
    redirect("/login?next=/security");
  }
  return (
    <FeaturePage
      className="content-stack"
      style={{ maxWidth: 800, margin: "40px auto", padding: 24 }}
    >
      <h1>Account security</h1><FeatureTabs/><WidgetGrid pageKey="account-security">
      <p>Manage your authenticator and activate protected emergency access.</p>
      <Link href="/my-access">View all my access</Link><FeatureWidget id="account-security-controls" title="Account security"><AccountSecurity /></FeatureWidget>
      <Link href="/">Return to your workspace</Link>
    </WidgetGrid></FeaturePage>
  );
}
