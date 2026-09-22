"use client";
import betaStyles from "@/components/beta/beta.module.css";
import { useState } from "react";
import { authClient } from "@/lib/auth-client";
import { Alert, Button, Field, TextInput } from "@/components/ui/foundation";
export function BetaAuthForm({
  mode,
  token,
  invalid = false,
}: {
  mode: "signup" | "forgot" | "reset" | "verify";
  token?: string;
  invalid?: boolean;
}) {
  const [message, setMessage] = useState(""),
    [failed, setFailed] = useState(false),
    [busy, setBusy] = useState(false),
    [completed, setCompleted] = useState(false);
  if (mode === "reset" && (!token || invalid))
    return (
      <div className={`beta-form ${betaStyles.surface}`}>
        <Alert tone="danger" title="Reset link unavailable">
          This link is invalid, expired, or has been replaced by a newer request.
        </Alert>
        <a href="/forgot-password">Request a new reset link</a>
        <a href="/login">Sign in</a>
      </div>
    );
  return (
    <form
      className={`beta-form ${betaStyles.surface}`}
      onSubmit={async (e) => {
        e.preventDefault();
        const form = e.currentTarget;
        setBusy(true);
        setMessage("");
        setFailed(false);
        const f = new FormData(form);
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
          setFailed(Boolean(result.error));
          if (!result.error && mode === "reset") {
            setCompleted(true);
            form.reset();
          }
          setMessage(
            result.error?.message ??
              (mode === "reset"
                ? "Password changed. You can sign in."
                : "Check your email for the next step."),
          );
        } catch {
          setFailed(true);
          setMessage("Unable to complete the request. Please try again.");
        } finally {
          setBusy(false);
        }
      }}
    >
      {mode === "signup" && (
        <Field label="Your name" required><TextInput name="name" required maxLength={100} autoComplete="name" /></Field>
      )}
      {mode !== "reset" && (
        <Field label="Email" required><TextInput type="email" name="email" required autoComplete="email" /></Field>
      )}
      {(mode === "signup" || mode === "reset") && !completed && (
        <Field label="Password" required hint="Use at least 12 characters.">
          <TextInput
            type="password"
            name="password"
            minLength={12}
            required
            autoComplete="new-password"
          />
        </Field>
      )}
      {!completed && <Button type="submit" pending={busy}>
        {mode === "signup"
            ? "Create account"
            : mode === "verify"
              ? "Resend verification link"
              : mode === "forgot"
                ? "Send reset link"
                : "Change password"}
      </Button>}
      {message && <Alert tone={failed ? "danger" : "success"} title={failed ? "Request not completed" : "Check the next step"} live>{message}</Alert>}
      <a href="/login">Sign in</a>
      {mode === "signup" && (
        <a href="/verify-email">Resend verification email</a>
      )}
    </form>
  );
}
