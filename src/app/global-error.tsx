"use client";
import { AccessState } from "@/components/workplace/access-state";
export default function GlobalError({ error, retry }: { error: Error & { digest?: string }; retry: () => void }) {
  return <html lang="en"><body style={{ margin: 0, fontFamily: "system-ui, sans-serif" }}><title>Temporarily unavailable · ERPX</title><AccessState problem="unavailable" reference={error.digest} retry={retry} /></body></html>;
}
