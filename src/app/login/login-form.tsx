"use client";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { authClient } from "@/lib/auth-client";
import { Alert, Button, Field, TextInput } from "@/components/ui/foundation";

export function LoginForm({ next, frontMode = false }: { next?: string; frontMode?: boolean }) {
  const router = useRouter();
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  async function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setError("");
    const form = new FormData(event.currentTarget);
    try {
      const result = await authClient.signIn.email({
        email: String(form.get("email")),
        password: String(form.get("password")),
        callbackURL: next ?? "/",
      });
      if (result.error) {
        setError(frontMode ? "เข้าสู่ระบบไม่สำเร็จ ตรวจสอบอีเมลและรหัสผ่าน แล้วลองอีกครั้ง" : result.error.message ?? "Sign in failed.");
        setPending(false);
        return;
      }
      router.push(next ?? "/");
      router.refresh();
    } catch {
      setError(frontMode ? "เชื่อมต่อไม่ได้ กรุณาลองเข้าสู่ระบบอีกครั้ง" : "Could not connect. Please try signing in again.");
    } finally {
      setPending(false);
    }
  }
  return (
    <form className="form" onSubmit={submit}>
      <Field label={frontMode ? "อีเมลพนักงาน" : "Work email"} required>
        <TextInput name="email" type="email" autoComplete="username" required />
      </Field>
      <Field label={frontMode ? "รหัสผ่าน" : "Password"} required>
        <TextInput
          name="password"
          type="password"
          autoComplete="current-password"
          required
        />
      </Field>
      {error && <Alert tone="danger" title={frontMode ? "เข้าสู่ระบบไม่สำเร็จ" : "Sign in failed"} live>{error}</Alert>}
      <Button type="submit" pending={pending}>{frontMode ? "เข้าสู่ระบบ" : "Sign in"}</Button>
    </form>
  );
}
