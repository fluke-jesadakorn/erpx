import betaStyles from "@/components/beta/beta.module.css";
import { BetaAuthForm } from "@/components/beta/auth-form";
import { runtimeEnv } from "@/server/runtime";
export const dynamic = "force-dynamic";
export default async function Signup() {
  const enabled = (await runtimeEnv()).ERPX_PUBLIC_BETA === "true";
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <p className="eyebrow">ERPX · Public beta</p>
      <h1>A shared home for your team’s work</h1>
      <p>
        Free during beta. No card required. Joining does not start a paid
        subscription.
      </p>
      {enabled ? (
        <BetaAuthForm mode="signup" />
      ) : (
        <p>
          Registration is not open yet. <a href="/login">Sign in</a>
        </p>
      )}
    </main>
  );
}
