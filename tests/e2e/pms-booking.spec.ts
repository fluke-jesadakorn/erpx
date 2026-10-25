import { expect, test } from "@playwright/test";
const isolated =
  process.env.ERPX_BOOKING_E2E === "1" &&
  new URL(process.env.ERPX_E2E_BASE_URL ?? "http://127.0.0.1:3100").port ===
    "3102";
test("guest portal is public without exposing internal PMS records", async ({
  page,
  request,
}) => {
  const slug = isolated ? "booking-test" : "acme";
  await page.goto(`/book/${slug}`);
  await expect(
    page.getByRole("heading", { name: "Find a stay that feels right." }),
  ).toBeVisible();
  await expect(
    page.getByRole("link", { name: "Manage booking" }),
  ).toBeVisible();
  const response = await request.post(`/api/public/booking/${slug}/manage`, {
    headers: {
      Origin: new URL(page.url()).origin,
      Authorization: "Bearer invalid",
    },
    data: {},
  });
  expect(response.status()).toBe(401);
  expect(JSON.stringify(await response.json())).not.toContain("guest@");
});
test("searches, quotes and confirms a family stay in an isolated hotel", async ({
  page,
}) => {
  test.skip(
    !isolated,
    "Requires the isolated booking fixture on port 3102; never creates a live reservation",
  );
  const arrival = new Date(); arrival.setUTCDate(arrival.getUTCDate() + 7);
  const departure = new Date(arrival); departure.setUTCDate(departure.getUTCDate() + 2);
  const cancellation = new Date(arrival); cancellation.setUTCDate(cancellation.getUTCDate() - 1);
  const arrivalDate = arrival.toISOString().slice(0, 10), departureDate = departure.toISOString().slice(0, 10), cancellationDate = cancellation.toISOString().slice(0, 10);
  await page.goto("/book/booking-test");
  await page.getByLabel("Check-in", { exact: true }).fill(arrivalDate);
  await page.getByLabel("Check-out", { exact: true }).fill(departureDate);
  await page.getByLabel("Child ages per room").fill("7");
  await page.getByRole("button", { name: "Search availability" }).click();
  await expect(page.getByText("THB 7,062.00", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Add rooms" }).click();
  await page.getByRole("button", { name: "Continue to guest details" }).click();
  await page.getByLabel("Full name").fill("Isolated browser guest");
  await page
    .getByLabel("Email", { exact: true })
    .fill("browser-qa@example.test");
  await page.getByLabel("Phone", { exact: true }).fill("020000000");
  await page.getByRole("button", { name: "Review price & policy" }).click();
  await expect(
    page.getByRole("heading", { name: "Review your stay" }),
  ).toBeVisible();
  await expect(
    page.getByText(`Free cancellation until ${cancellationDate} 18:00 (Asia/Bangkok). After this deadline, contact the hotel. Pay at the hotel.`, { exact: true }),
  ).toBeVisible();
  await page
    .getByLabel("I have reviewed the dates, full price and cancellation terms.")
    .check();
  await page.getByRole("button", { name: "Confirm reservation", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: /Booking confirmed · PMS-/ }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
});
