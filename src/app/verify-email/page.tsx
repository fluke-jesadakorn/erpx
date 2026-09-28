import { BetaAuthForm } from "@/components/beta/auth-form";
import { AuthShell } from "@/components/workplace/auth-shell";
export default function VerifyEmail() {
  return (
    <AuthShell eyebrow="ACCOUNT ACCESS" title="Verify your email." description="Request a fresh verification link for your account.">
      <BetaAuthForm mode="verify" />
    </AuthShell>
  );
}
