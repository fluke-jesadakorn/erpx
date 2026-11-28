import { BetaAuthForm } from "@erpx/collaboration-ui/components/auth-form";
import { AuthShell } from "@erpx/platform-workplace/components/auth-shell";
export default function Forgot() {
  return (
    <AuthShell eyebrow="ACCOUNT RECOVERY" title="Reset your password." description="Enter your work email and we’ll send you a secure link to continue.">
      <BetaAuthForm appearance="standard" mode="forgot" />
    </AuthShell>
  );
}
