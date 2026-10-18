"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import dynamic from "next/dynamic";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import styles from "./pos.module.css";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {BackofficePosConsoleContext} from "./standard-pos-backoffice-console";



const Settings = dynamic(() => import("./pos-management").then(module => module.Settings));
export function BackofficePosConsoleSettingsScreen({context}: {context: BackofficePosConsoleContext}) {
  const {base, data, busy, send, dailyPosRoot, financeMode, refresh, can} = context;
  return <FrontLocalizedTree>{(
                <>
                  <UiActionLink
                    variant="primary"
                    className={styles.button}
                    href={canonicalUiHref(`${base}/finance/hospitality-settings`)}
                  >
                    Hotel + Restaurant Setup
                  </UiActionLink>
                  <Settings
                    data={data}
                    base={base}
                    busy={busy}
                    send={send}
                    dailyPosRoot={dailyPosRoot}
                    storeMode={!financeMode}
                    refresh={refresh}
                  />
                  {(can("pos.payments.configure") || can("finance.approve")) && (
                    <UiActionLink
                      variant="primary"
                      className={styles.button}
                      href={canonicalUiHref(`${base}/settings/stripe`)}
                    >
                      Company Stripe · POS and PMS payments
                    </UiActionLink>
                  )}
                </>
              )}</FrontLocalizedTree>;
}
