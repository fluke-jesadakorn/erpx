import Link from "next/link";
import { currentSession } from "@/server/authorization";
import { redirect } from "next/navigation";
import { AccountSecurity } from "./security-form";
export default async function SecurityPage() {
  try {
    await currentSession();
  } catch {
    redirect("/login?next=/security");
  }
  return (
    <main
      className="content-stack"
      style={{ maxWidth: 800, margin: "40px auto", padding: 24 }}
    >
      <h1>Account security</h1>
      <p>Manage your authenticator and activate protected emergency access.</p>
      <Link href="/my-access">View all my access</Link><AccountSecurity />
      <Link href="/">Return to your workspace</Link>
    </main>
  );
}
