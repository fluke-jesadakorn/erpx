"use client";
import { useState, useSyncExternalStore } from "react";
import { useRouter } from "next/navigation";
import { authClient } from "@/lib/auth-client";
import { Alert, Button, Field, FormLayout, TextInput, type UiAppearance } from "@/components/ui/foundation";
const subscribeHydration = () => () => {};
const clientHydrated = () => true;
const serverHydrated = () => false;

export function LoginForm({ next, frontMode = false, appearance = "legacy" }: { next?: string; frontMode?: boolean; appearance?: UiAppearance }) {
  const router = useRouter();
  const hydrated = useSyncExternalStore(subscribeHydration, clientHydrated, serverHydrated);
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
  const content = (
    <>
      <Field appearance={appearance} label={frontMode ? "อีเมลพนักงาน" : "Work email"} required>
        <TextInput appearance={appearance} name="email" type="email" autoComplete="username" required />
      </Field>
      <Field appearance={appearance} label={frontMode ? "รหัสผ่าน" : "Password"} required>
        <TextInput appearance={appearance}
          name="password"
          type="password"
          autoComplete="current-password"
          required
        />
      </Field>
      {error && <Alert tone="danger" title={frontMode ? "เข้าสู่ระบบไม่สำเร็จ" : "Sign in failed"} live>{error}</Alert>}
      <Button appearance={appearance} clientManaged type="submit" pending={pending} disabled={!hydrated}>{frontMode ? "เข้าสู่ระบบ" : "Sign in"}</Button>
    </>
  );
  return appearance === "standard" ? <FormLayout layout="stack" method="post" onSubmit={submit}>{content}</FormLayout> : <form className="form" method="post" onSubmit={submit}>{content}</form>;
}
