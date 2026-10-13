"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useState } from "react";
import { useRouter } from "next/navigation";
import { FormLayout } from "@/components/ui/foundation";
export function ActivationForm({ token, existingAccount = false }: { token: string; existingAccount?: boolean }) {
  const router = useRouter();
  const [error, setError] = useState("");
  const [pending, setPending] = useState(false);
  return (
    <FormLayout
      className="ui-form-grid"
      onSubmit={async (e) => {
        e.preventDefault();
        const f = new FormData(e.currentTarget);
        if (!existingAccount && f.get("password") !== f.get("confirm")) {
          setError("Passwords must match.");
          return;
        }
        setPending(true);
        setError("");
        try {
          const r = await workFetch("/api/v1/account-invitations", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ token, password: f.get("password") }),
          });
          const v = (await r.json()) as { error?: { message: string } };
          if (!r.ok) throw new Error(v.error?.message ?? "Activation failed.");
          router.replace("/login");
        } catch (e) {
          setError(e instanceof Error ? e.message : "Activation failed.");
        } finally {
          setPending(false);
        }
      }}
    >
      <Field appearance="standard">
        {existingAccount ? "Existing account password" : "Password"}
        <TextInput appearance="standard"
          name="password"
          type="password"
          minLength={existingAccount ? 1 : 12}
          maxLength={128}
          autoComplete={existingAccount ? "current-password" : "new-password"}
          required
        />
      </Field>
      {!existingAccount && <Field appearance="standard">
        Confirm password
        <TextInput appearance="standard"
          name="confirm"
          type="password"
          minLength={12}
          maxLength={128}
          autoComplete="new-password"
          required
        />
      </Field>}
      <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={pending}>
        Activate account
      </Button>
      {error && (
        <InlineNotice tone="danger" className="error" role="alert">
          {error}
        </InlineNotice>
      )}
    </FormLayout>
  );
}
