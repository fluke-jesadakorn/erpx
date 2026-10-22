import { cookies, headers } from "next/headers";
import { notFound } from "next/navigation";
import { productHostForHost } from "@erpx/feature-runtime/lib/product-hosts";
import { preferredAccountLanguage } from "@erpx/server-identity_access/public/account-preference-snapshot";
import { ProductBrand } from "@erpx/feature-brand/components/product-brand";
import { PageContent, Stack } from "@erpx/ui/components/foundation";
import styles from "@erpx/workplace/components/product-shell.module.css";

const welcomes = {
  portal: ["Customer Portal", "Use the document link shared with you."],
  "legal-portal": ["Legal Portal", "Use your verified document invitation."],
  forms: ["Forms", "Use the published form link supplied by the company."],
} as const;
export default async function PublicEntry() {
  const host = productHostForHost((await headers()).get("host"));
  if (!host || !(host in welcomes)) notFound();
  // Keep the existing account preference read for consistent request behavior.
  await preferredAccountLanguage((await cookies()).get("erpx_language")?.value === "th" ? "th" : "en");
  const [title, message] = welcomes[host as keyof typeof welcomes];
  return <PageContent as="main"><Stack><ProductBrand appearance="standard" className={styles.mark} variant="symbol" /><h1>{title}</h1><p>{message}</p></Stack></PageContent>;
}
