
import betaStyles from "@/components/beta/beta.module.css";
import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { createAuth } from "@/server/modules/identity_access/public/auth";

import { OnboardingForm } from "@/components/beta/onboarding-form";
import { onboardingPresentationQueries } from "@/server/modules/manage_erpx/public/onboarding-presentation-query";
export const dynamic = "force-dynamic";
export default async function Onboarding() {
  const session = await (
    await createAuth()
  ).api.getSession({ headers: await headers() });
  if (!session) redirect("/login");
  const presentation = await onboardingPresentationQueries();
  const slot = await presentation.slot(session.user.id);
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
      ) : presentation.publicBetaEnabled ? (
        <OnboardingForm initial={slot} />
      ) : (
        <p>Public registration is closed.</p>
      )}
    </main>
  );
}
