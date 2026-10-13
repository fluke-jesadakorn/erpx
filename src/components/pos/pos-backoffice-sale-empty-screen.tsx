"use client";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import styles from "./pos.module.css";
import { FrontLocalizedTree } from "@/components/front/front-language";
import type {BackofficePosConsoleContext} from "./pos-backoffice-console";

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
                      href={`${erpBase}/commerce`}
                    >
                      Open Commerce products
                    </UiActionLink>
                  </section>
                )}</FrontLocalizedTree>;
}
