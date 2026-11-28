"use client";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import dynamic from "next/dynamic";
import { ActionLink as UiActionLink } from "@erpx/platform-ui/components/foundation";
import styles from "@erpx/pos-ui/components/pos.module.css";
import { FrontLocalizedTree } from "@erpx/platform-front/components/front-language";
import type {BackofficePosConsoleContext} from "@erpx/pos-ui/components/standard-pos-backoffice-console";



const Settings = dynamic(() => import("@erpx/pos-ui/components/pos-management").then(module => module.Settings));
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
