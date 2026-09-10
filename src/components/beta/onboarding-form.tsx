"use client";
import betaStyles from "@/components/beta/beta.module.css";
import { useState } from "react";
export function OnboardingForm({
  initial,
}: {
  initial?: { slug: string; company_name: string; branch_name: string } | null;
}) {
  const [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false);
  return (
    <form
      className={`beta-form ${betaStyles.surface}`}
      onSubmit={async (e) => {
        e.preventDefault();
        const form = new FormData(e.currentTarget);
        setBusy(true);
        try {
          const response = await fetch("/api/v1/onboarding", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(Object.fromEntries(form)),
          });
          const result = (await response.json()) as {
            error: { message: string };
            data: { href: string | null };
          };
          if (!response.ok) throw new Error(result.error.message);
          if (result.data.href) window.location.assign(result.data.href);
          else
            setMessage(
              "The first 20 company spaces are full. You are on the waiting list.",
            );
        } catch (e) {
          setMessage(e instanceof Error ? e.message : "Please try again.");
        } finally {
          setBusy(false);
        }
      }}
    >
      <label>
        Company name
        <input
          name="companyName"
          defaultValue={initial?.company_name}
          required
          minLength={2}
          maxLength={100}
          readOnly={!!initial}
        />
      </label>
      <label>
        Company URL
        <input
          name="slug"
          defaultValue={initial?.slug}
          pattern="[a-z][a-z0-9\-]{2,39}"
          required
          readOnly={!!initial}
        />
      </label>
      <label>
        First branch
        <input
          name="branchName"
          defaultValue={initial?.branch_name}
          required
          minLength={2}
          maxLength={100}
          readOnly={!!initial}
        />
      </label>
      <button disabled={busy}>
        {busy
          ? "Preparing your workspace…"
          : initial
            ? "Resume setup"
            : "Create my workspace"}
      </button>
      <p role="status">{message}</p>
    </form>
  );
}
