"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import dynamic from "next/dynamic";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree } from "@erpx/front/components/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";



const Settings = dynamic(() => import("@erpx/feature-pos/components/pos-management").then(module => module.Settings));
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
