import { expect, test } from "@playwright/test";

for (const path of ["/holdings/acme/pms", "/holdings/acme/pms/housekeeping"]) {
  test(`PMS protects ${path} before exposing guest or room data`, async ({
    page,
  }) => {
    await page.goto(path);
    await expect(page).toHaveURL(/\/login(?:\?.*)?$/);
    await expect(
      page.getByRole("heading", { name: "Welcome back" }),
    ).toBeVisible();
    await expect(
      page.getByRole("navigation", { name: "งานโรงแรม" }),
    ).toHaveCount(0);
  });
}

test("PMS commands require an authenticated work context", async ({
  request,
}) => {
  const response = await request.post("/api/v1/holdings/acme/pms", {
    data: {
      operation: "night.start",
      propertyId: "not-a-property",
      expectedVersion: 1,
      input: {},
    },
    headers: { "Idempotency-Key": "e2e-pms-unauthenticated" },
  });
  expect(response.status()).toBe(401);
  expect((await response.json()).error.code).toBe("UNAUTHENTICATED");
});
