"use client";
import betaStyles from "@erpx/feature-collaboration/components/beta.module.css";
import { useState } from "react";
import { authClient } from "@erpx/feature-identity_access/lib/auth-client";
import { Alert, Button, Field, TextInput } from "@erpx/ui/components/foundation";
import { type UiAppearance } from "@erpx/ui/components/foundation";
export function BetaAuthForm({ appearance = "legacy",
  mode,
  token,
  invalid = false,
  frontMode = false,
}: ({
  mode: "signup" | "forgot" | "reset" | "verify";
  token?: string;
  invalid?: boolean;
  frontMode?: boolean;
}) & { appearance?: UiAppearance }) {
  const [message, setMessage] = useState(""),
    [failed, setFailed] = useState(false),
    [busy, setBusy] = useState(false),
    [completed, setCompleted] = useState(false);
  if (mode === "reset" && (!token || invalid))
    return (
      <div data-ui-appearance={appearance} className={`beta-form ${betaStyles.surface}`}>
        <Alert tone="danger" title="Reset link unavailable">
          This link is invalid, expired, or has been replaced by a newer request.
        </Alert>
        <a data-ui-appearance={appearance} href="/forgot-password">Request a new reset link</a>
        <a data-ui-appearance={appearance} href="/login">Sign in</a>
      </div>
    );
  return (
    <form data-ui-appearance={appearance}
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
                  callbackURL: frontMode ? "/" : "/onboarding",
                })
              : mode === "verify"
                ? await authClient.sendVerificationEmail({
                    email,
                    callbackURL: frontMode ? "/" : "/onboarding",
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
            frontMode && result.error ? "The request could not be completed. Check your details and try again." : result.error?.message ??
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
        <Field appearance={appearance} label="Your name" required><TextInput appearance={appearance} name="name" required maxLength={100} autoComplete="name" /></Field>
      )}
      {mode !== "reset" && (
        <Field appearance={appearance} label="Email" required><TextInput appearance={appearance} type="email" name="email" required autoComplete="email" /></Field>
      )}
      {(mode === "signup" || mode === "reset") && !completed && (
        <Field appearance={appearance} label={frontMode ? "New password" : "Password"} required hint="Use at least 12 characters.">
          <TextInput appearance={appearance}
            type="password"
            name="password"
            minLength={12}
            required
            autoComplete="new-password"
          />
        </Field>
      )}
      {!completed && <Button appearance={appearance} clientManaged type="submit" pending={busy}>
        {mode === "signup"
            ? "Create account"
            : mode === "verify"
              ? "Resend verification link"
              : mode === "forgot"
                ? "Send reset link"
                : "Change password"}
      </Button>}
      {message && <Alert tone={failed ? "danger" : "success"} title={failed ? "Request not completed" : "Check the next step"} live>{message}</Alert>}
      <a data-ui-appearance={appearance} href="/login">Sign in</a>
      {mode === "signup" && (
        <a data-ui-appearance={appearance} href="/verify-email">Resend verification email</a>
      )}
    </form>
  );
}
