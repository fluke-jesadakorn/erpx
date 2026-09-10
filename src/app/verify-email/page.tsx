import betaStyles from "@/components/beta/beta.module.css";
import { BetaAuthForm } from "@/components/beta/auth-form";
export default function VerifyEmail() {
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <h1>Verify your email</h1>
      <p>Request a new verification link for your account.</p>
      <BetaAuthForm mode="verify" />
    </main>
  );
}
