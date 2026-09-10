import betaStyles from "@/components/beta/beta.module.css";
import { BetaAuthForm } from "@/components/beta/auth-form";
export default function Forgot() {
  return (
    <main className={`beta-entry ${betaStyles.surface}`}>
      <h1>Reset your password</h1>
      <BetaAuthForm mode="forgot" />
    </main>
  );
}
