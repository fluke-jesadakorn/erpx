"use client";
import { useEffect, useRef } from "react";

/** SSO starts through a document navigation so a streamed RSC redirect cannot
 * race another start and overwrite the browser-bound transaction cookie. */
export function FrontSignInHandoff({ next }: { next: string }) {
  const started = useRef(false);
  const href = "/login?" + new URLSearchParams({ next });
  useEffect(() => {
    if (started.current) return;
    started.current = true;
    window.location.replace(href);
  }, [href]);
  return <main><p>Sign in to continue.</p><a href={href}>Sign in</a></main>;
}
