import betaStyles from "@/components/beta/beta.module.css";
import { BetaAuthForm } from "@/components/beta/auth-form";
export default async function Reset({
  searchParams,
}: {
  searchParams: Promise<{ token?: string; error?: string }>;
}) {
  const query = await searchParams;
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <h1>Choose a new password</h1>
      <BetaAuthForm mode="reset" token={query.token} invalid={Boolean(query.error)} />
    </main>
  );
}
