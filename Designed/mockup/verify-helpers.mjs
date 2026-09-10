/* Browser helpers exercise visible disclosure controls rather than bypassing them. */
export async function reveal(locator) {
  await locator.waitFor({ state: "attached" });
  const ancestors = await locator.locator("xpath=ancestor::details").all();
  for (const detail of ancestors) {
    if (
      !(await detail.getAttribute("open")) &&
      !(await detail.evaluate((el) => el.open))
    )
      await detail.locator(":scope > summary").click();
  }
}
export async function clickAction(page, action, root = page, id) {
  if (
    action === "demo" &&
    (await page.locator("dialog").evaluate((d) => d.open))
  )
    await page.locator('dialog [data-action="close-dialog"]').first().click();
  let button = root
    .locator(
      `[data-action="${action}"]${id === undefined ? "" : `[data-id="${id}"]`}`,
    )
    .first();
  if (action === "role" && !(await button.isVisible())) {
    await page.locator(".context-control").click();
    button = page.locator('dialog [data-action="role"]');
  }
  if (
    ["wf-suggest", "wf-apply-suggestion", "wf-reject-suggestion"].includes(
      action,
    ) &&
    !(await button.isVisible())
  ) {
    await clickAction(page, "task-assistant");
    button = page.locator(`dialog [data-action="${action}"]`).first();
  }
  if (action === "task-continue" && !(await button.count())) return;
  if (action === "cx-picker") {
    const editor = await button.getAttribute("data-editor");
    const input = page.locator("#" + editor);
    await reveal(input);
    await input.focus();
  }
  await reveal(button);
  await button.click();
}

export async function clearDemoStorage(page) {
  await page.evaluate(async () => {
    if (window.ERPX_STORAGE) await window.ERPX_STORAGE.clear();
  });
}
