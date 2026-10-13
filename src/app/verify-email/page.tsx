import { BetaAuthForm } from "@/components/collaboration/auth-form";
import { AuthShell } from "@/components/workplace/auth-shell";
export default function VerifyEmail() {
  return (
    <AuthShell eyebrow="ACCOUNT ACCESS" title="Verify your email." description="Request a fresh verification link for your account.">
      <BetaAuthForm appearance="standard" mode="verify" />
    </AuthShell>
  );
}
