import { BetaAuthForm } from "@erpx/feature-collaboration/components/auth-form";
import { AuthShell } from "@erpx/workplace/components/auth-shell";
import { publicBetaEnabled } from "@erpx/server-runtime/public/presentation-configuration";
export const dynamic = "force-dynamic";
export default async function Signup() {
  const enabled = await publicBetaEnabled();
  return (
    <AuthShell eyebrow="EARLY ACCESS" title={enabled ? "Create your account." : "Registration is closed."} description={enabled ? "Join the early-access workspace. No card required." : "New accounts are not open at this time. If you already have access, sign in to continue."}>
      {enabled ? (
        <BetaAuthForm appearance="standard" mode="signup" />
      ) : (
        <p><a href="/login">Sign in to your account</a></p>
      )}
    </AuthShell>
  );
}
