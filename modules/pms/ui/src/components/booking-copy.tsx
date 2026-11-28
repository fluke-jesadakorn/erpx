"use client";

import { useFrontLanguage } from "@erpx/platform-front/components/front-language";
import { frontText } from "@erpx/platform-shared-ui/lib/front-language";

/** Page copy resolves inside the guest language context, including RSC pages. */
export function BookingCopy({
  as: Tag = "span",
  children,
  className,
}: {
  as?: "span" | "h1" | "h2" | "p";
  children: string;
  className?: string;
}) {
  const language = useFrontLanguage();
  return <Tag className={className}>{frontText(children, language)}</Tag>;
}
