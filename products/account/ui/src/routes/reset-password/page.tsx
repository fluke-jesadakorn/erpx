import { BetaAuthForm } from "@erpx/collaboration-ui/components/auth-form";
import { AuthShell } from "@erpx/platform-workplace/components/auth-shell";
export default async function Reset({
  searchParams,
}: {
  searchParams: Promise<{ token?: string; error?: string }>;
}) {
  const query = await searchParams;
  return (
    <AuthShell eyebrow="ACCOUNT RECOVERY" title="Choose a new password." description="Use a strong password to protect your ERPX workspace.">
      <BetaAuthForm appearance="standard" mode="reset" token={query.token} invalid={Boolean(query.error)} />
    </AuthShell>
  );
}
