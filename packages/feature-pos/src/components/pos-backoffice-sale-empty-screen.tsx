"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { FrontLocalizedTree } from "@erpx/front/components/front-language";
import type {BackofficePosConsoleContext} from "@erpx/feature-pos/components/standard-pos-backoffice-console";

export function BackofficePosConsoleSaleEmptyScreen({context}: {context: BackofficePosConsoleContext}) {
  const {erpBase} = context;
  return <FrontLocalizedTree>{(
                  <section className={styles.setupCard}>
                    <div>
                      <p className={styles.eyebrow}>Next step</p>
                      <h2>Add a product</h2>
                      <p>
                        POS uses the active products already in Commerce. Add at least one product
                        before opening your first sale.
                      </p>
                    </div>
                    <UiActionLink
                      variant="primary"
                      className={styles.button}
                      href={canonicalUiHref(`${erpBase}/commerce`)}
                    >
                      Open Commerce products
                    </UiActionLink>
                  </section>
                )}</FrontLocalizedTree>;
}
