"use client";

import { useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";

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
