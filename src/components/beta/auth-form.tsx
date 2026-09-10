"use client";
import betaStyles from "@/components/beta/beta.module.css";
import { useState } from "react";
import { authClient } from "@/lib/auth-client";
export function BetaAuthForm({
  mode,
  token,
}: {
  mode: "signup" | "forgot" | "reset" | "verify";
  token?: string;
}) {
  const [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false);
  return (
    <form
      className={`beta-form ${betaStyles.surface}`}
      onSubmit={async (e) => {
        e.preventDefault();
        setBusy(true);
        setMessage("");
        const f = new FormData(e.currentTarget);
        try {
          const email = String(f.get("email") ?? "");
          const result =
            mode === "signup"
              ? await authClient.signUp.email({
                  name: String(f.get("name")),
                  email,
                  password: String(f.get("password")),
                  callbackURL: "/onboarding",
                })
              : mode === "verify"
                ? await authClient.sendVerificationEmail({
                    email,
                    callbackURL: "/onboarding",
                  })
                : mode === "forgot"
                  ? await authClient.requestPasswordReset({
                      email,
                      redirectTo: "/reset-password",
                    })
                  : await authClient.resetPassword({
                      newPassword: String(f.get("password")),
                      token: token ?? "",
                    });
          setMessage(
            result.error?.message ??
              (mode === "reset"
                ? "Password changed. You can sign in."
                : "Check your email for the next step."),
          );
        } catch {
          setMessage("Unable to complete the request. Please try again.");
        } finally {
          setBusy(false);
        }
      }}
    >
      {mode === "signup" && (
        <label>
          Your name
          <input name="name" required maxLength={100} autoComplete="name" />
        </label>
      )}
      {mode !== "reset" && (
        <label>
          Email
          <input type="email" name="email" required autoComplete="email" />
        </label>
      )}
      {(mode === "signup" || mode === "reset") && (
        <label>
          Password
          <input
            type="password"
            name="password"
            minLength={12}
            required
            autoComplete="new-password"
          />
        </label>
      )}
      <button disabled={busy} className="primary-button">
        {busy
          ? "Please wait…"
          : mode === "signup"
            ? "Create account"
            : mode === "verify"
              ? "Resend verification link"
              : mode === "forgot"
                ? "Send reset link"
                : "Change password"}
      </button>
      <p role="status">{message}</p>
      <a href="/login">Sign in</a>
      {mode === "signup" && (
        <a href="/verify-email">Resend verification email</a>
      )}
    </form>
  );
}
