import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";

import betaStyles from "@erpx/feature-collaboration/components/beta.module.css";
import { headers } from "next/headers";
import { redirect } from "next/navigation";
import { createAuth } from "@erpx/server-identity_access/public/auth";

import { OnboardingForm } from "@erpx/feature-collaboration/components/onboarding-form";
import { nativeManageErpxOnboardingViewForPage } from "../../lib/native-manage-erpx";
export const dynamic = "force-dynamic";
export default async function Onboarding() {
  const session = await (
    await createAuth()
  ).api.getSession({ headers: await headers() });
  if (!session) redirect("/login");
  const verified = session.user.emailVerified;
  const view = verified
    ? await nativeManageErpxOnboardingViewForPage(session.user.id).catch(() => undefined)
    : undefined;
  const slot = view?.slot ?? null;
  if (slot?.state === "active") redirect(canonicalUiHref(`/holdings/${slot.slug}/home`));
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <p className="eyebrow">Welcome to ERPX</p>
      <h1>Bring your team together</h1>
      <p>
        Start with your company and first branch, then invite your team. Your
        workspace starts empty.
      </p>
      {!verified ? (
        <p>Open the verification link in your email before continuing.</p>
      ) : !view ? (
        <p>Company onboarding is temporarily unavailable. Try again shortly.</p>
      ) : view.enabled ? (
        <OnboardingForm
          initial={
            slot
              ? {
                  slug: slot.slug,
                  company_name: slot.company_name,
                  branch_name: slot.branch_name,
                }
              : null
          }
        />
      ) : (
        <p>Public registration is closed.</p>
      )}
    </main>
  );
}
