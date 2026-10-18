"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work_projection/work-fetch";
import { useRouter } from "@/lib/shared/destination-router";
import { useState } from "react";
export function JoinForm({ holding, token }: { holding: string; token: string }) {
  const router = useRouter();
  const [message, setMessage] = useState(""),
    [busy, setBusy] = useState(false);
  return (
    <div>
      <Button appearance="standard"
        type="submit"
        variant="ghost"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          try {
            const r = await workFetch("/api/v1/join", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ holding, token }),
            });
            const body = (await r.json()) as { error?: { message: string } };
            if (!r.ok) throw new Error(body.error?.message);
            router.push("/");
          } catch (e) {
            setMessage(e instanceof Error ? e.message : "Please try again.");
          } finally {
            setBusy(false);
          }
        }}
      >
        Join this company
      </Button>
      <InlineNotice tone="info" role="status">{message}</InlineNotice>
      <p>
        Use the verified email that received this invitation. <a href="/login">Sign in</a> or{" "}
        <a href="/signup">create an account</a>, then return to this link.
      </p>
    </div>
  );
}
