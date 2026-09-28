"use client";
import betaStyles from "@/components/beta/beta.module.css";
import { useState } from "react";
import { authClient } from "@/lib/auth-client";
import { Alert, Button, Field, TextInput } from "@/components/ui/foundation";
export function BetaAuthForm({
  mode,
  token,
  invalid = false,
  frontMode = false,
}: {
  mode: "signup" | "forgot" | "reset" | "verify";
  token?: string;
  invalid?: boolean;
  frontMode?: boolean;
}) {
  const [message, setMessage] = useState(""),
    [failed, setFailed] = useState(false),
    [busy, setBusy] = useState(false),
    [completed, setCompleted] = useState(false);
  if (mode === "reset" && (!token || invalid))
    return (
      <div className={`beta-form ${betaStyles.surface}`}>
        <Alert tone="danger" title={frontMode ? "ลิงก์ใช้ไม่ได้" : "Reset link unavailable"}>
          {frontMode ? "ลิงก์นี้หมดอายุหรือถูกแทนด้วยคำขอใหม่" : "This link is invalid, expired, or has been replaced by a newer request."}
        </Alert>
        <a href="/forgot-password">{frontMode ? "ขอลิงก์ใหม่" : "Request a new reset link"}</a>
        <a href="/login">{frontMode ? "เข้าสู่ระบบ" : "Sign in"}</a>
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
            frontMode && result.error ? "ดำเนินการไม่สำเร็จ กรุณาตรวจสอบข้อมูลแล้วลองอีกครั้ง" : result.error?.message ??
              (mode === "reset"
                ? frontMode ? "เปลี่ยนรหัสผ่านแล้ว เข้าสู่ระบบได้เลย" : "Password changed. You can sign in."
                : frontMode ? "ตรวจสอบอีเมลเพื่อทำขั้นตอนต่อไป" : "Check your email for the next step."),
          );
        } catch {
          setFailed(true);
          setMessage(frontMode ? "เชื่อมต่อไม่ได้ กรุณาลองอีกครั้ง" : "Unable to complete the request. Please try again.");
        } finally {
          setBusy(false);
        }
      }}
    >
      {mode === "signup" && (
        <Field label="Your name" required><TextInput name="name" required maxLength={100} autoComplete="name" /></Field>
      )}
      {mode !== "reset" && (
        <Field label={frontMode ? "อีเมล" : "Email"} required><TextInput type="email" name="email" required autoComplete="email" /></Field>
      )}
      {(mode === "signup" || mode === "reset") && !completed && (
        <Field label={frontMode ? "รหัสผ่านใหม่" : "Password"} required hint={frontMode ? "อย่างน้อย 12 ตัวอักษร" : "Use at least 12 characters."}>
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
              ? frontMode ? "ส่งลิงก์ยืนยันอีกครั้ง" : "Resend verification link"
              : mode === "forgot"
                ? frontMode ? "ส่งลิงก์กู้รหัสผ่าน" : "Send reset link"
                : frontMode ? "เปลี่ยนรหัสผ่าน" : "Change password"}
      </Button>}
      {message && <Alert tone={failed ? "danger" : "success"} title={frontMode ? failed ? "ดำเนินการไม่สำเร็จ" : "ขั้นตอนถัดไป" : failed ? "Request not completed" : "Check the next step"} live>{message}</Alert>}
      <a href="/login">{frontMode ? "เข้าสู่ระบบ" : "Sign in"}</a>
      {mode === "signup" && (
        <a href="/verify-email">Resend verification email</a>
      )}
    </form>
  );
}
