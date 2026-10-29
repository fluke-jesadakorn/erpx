import { expect, test } from "@playwright/test";
import { businessActor, businessFixture, closeBusiness } from "../../support/browser/engineering-business";

test("Maintenance preventive plan editor opens once after hydration and closes without changes", async ({ browser }, testInfo) => {
  test.skip(!process.env.ERPX_DEMO_RUN, "Set ERPX_DEMO_RUN to an exact prepared browser-simulation run.");
  test.setTimeout(120_000);
  const loaded = await businessFixture(testInfo);
  const contexts: import("@playwright/test").BrowserContext[] = [];
  try {
    const actor = await businessActor(browser, testInfo, loaded, "source", "/holdings/simulation/services/assets/plans");
    contexts.push(actor.context);
    const { page } = actor;
    const writes: string[] = [];
    page.on("request", request => {
      if (request.method() !== "GET" && new URL(request.url()).pathname === "/api/v1/holdings/simulation/maintenance/plans") writes.push(request.method());
    });

    const trigger = page.getByRole("button", { name: "Create preventive plan", exact: true });
    await expect(trigger).toHaveAttribute("data-client-managed-ready", "true");
    await expect(trigger).toBeEnabled();
    await trigger.click();
    const drawer = page.getByRole("dialog", { name: "Create preventive plan", exact: true });
    await expect(drawer).toHaveCount(1);
    await expect(drawer).toBeVisible();
    await expect(drawer).toHaveAttribute("data-presentation", "form-drawer");
    await expect.poll(() => drawer.evaluate(element => {
      const bounds = element.getBoundingClientRect();
      return Math.abs(bounds.right - window.innerWidth) < 1;
    })).toBe(true);
    await expect(drawer.getByRole("combobox", { name: "Asset", exact: true })).toBeFocused();
    await drawer.getByRole("button", { name: "Close dialog", exact: true }).click();
    await expect(drawer).not.toBeVisible();
    await expect(trigger).toBeFocused();
    expect(writes).toEqual([]);
  } finally {
    await closeBusiness(contexts, loaded.db);
  }
});
