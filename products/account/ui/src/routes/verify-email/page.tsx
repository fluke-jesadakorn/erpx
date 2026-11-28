import { BetaAuthForm } from "@erpx/collaboration-ui/components/auth-form";
import { AuthShell } from "@erpx/platform-workplace/components/auth-shell";
export default function VerifyEmail() {
  return (
    <AuthShell eyebrow="ACCOUNT ACCESS" title="Verify your email." description="Request a fresh verification link for your account.">
      <BetaAuthForm appearance="standard" mode="verify" />
    </AuthShell>
  );
}
