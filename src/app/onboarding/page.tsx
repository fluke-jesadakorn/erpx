import betaStyles from "@/components/beta/beta.module.css";
import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { createAuth } from "@/server/auth";
import { runtimeEnv } from "@/server/runtime";
import { OnboardingForm } from "@/components/beta/onboarding-form";
export const dynamic = "force-dynamic";
export default async function Onboarding() {
  const session = await (
    await createAuth()
  ).api.getSession({ headers: await headers() });
  if (!session) redirect("/login");
  const env = await runtimeEnv();
  const slot = await env.CONTROL_DB.prepare(
    "SELECT slug,company_name,branch_name,state FROM beta_slots WHERE owner_user_id=?",
  )
    .bind(session.user.id)
    .first<{
      slug: string;
      company_name: string;
      branch_name: string;
      state: string;
    }>();
  if (slot?.state === "active") redirect(`/holdings/${slot.slug}/home`);
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <p className="eyebrow">Welcome to ERPX</p>
      <h1>Bring your team together</h1>
      <p>
        Start with your company and first branch, then invite your team. Your
        workspace starts empty.
      </p>
      {!session.user.emailVerified ? (
        <p>Open the verification link in your email before continuing.</p>
      ) : env.ERPX_PUBLIC_BETA === "true" ? (
        <OnboardingForm initial={slot} />
      ) : (
        <p>Public registration is closed.</p>
      )}
    </main>
  );
}
