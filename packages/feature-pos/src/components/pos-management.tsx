"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button as UiButton, Field, Select, TextInput } from "@erpx/ui/components/foundation";
import { FeatureWidget as FeatureWidgetBase } from "@erpx/feature-shared/components/features/feature-page";
import { accountKeys, type PosAccounts } from "@erpx/server-pos/public/rules/pos";
import { posCommerceAccountDefaults, posGeneratedAccountSpecs } from "@erpx/feature-pos/lib/pos-bootstrap";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { PosWorkspace } from "@erpx/server-pos/public/pos";
import type {PosRow} from '@erpx/server-pos/public/pos-core';
import Link from "@erpx/workplace/components/workspace-link";
import { useRef, useState, type ComponentProps, type FormEvent, type ReactNode } from "react";
import styles from "@erpx/feature-pos/components/pos.module.css";
import { useFrontLanguage } from "@erpx/front/components/front-language";
import { frontText } from "@erpx/feature-shared/lib/front-language";
type Row = PosRow;

const s = (r: Row, k: string) => String(r[k] ?? "");

const n = (r: Row, k: string) => Number(r[k] ?? 0);

const fields = (e: FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  return new FormData(e.currentTarget);
};

const text = (f: FormData, k: string) => String(f.get(k) ?? "");

const labels: Record<string, string> = {
  open: "Open",
  held: "Held",
  payment_pending: "Payment pending",
  completed: "Succeeded",
  cancelled: "Cancelled",
  merged: "Merged",
  draft: "Not sent",
  queued: "Kitchen pending",
  preparing: "In progress",
  ready: "Ready",
  delivered: "Delivered",
  requested: "Pending",
  pending: "Awaiting result",
  succeeded: "Succeeded",
  failed: "Failed",
  exception: "Needs review",
  closed: "Cash session closed",
  reviewed: "Reviewed",
  approved: "Approved",
  rejected: "Rejected",
  processing: "Refunding",
  onboarding: "Onboarding",
  unconfigured: "Not configured",
  active: "Active",
  inactive: "Inactive",
  cash: "Cash",
  transfer: "Bank transfer",
  edc: "External EDC",
  stripe_card: "Stripe card",
  stripe_promptpay: "PromptPay / Stripe",
};

const kitchenLabels: Record<string, [string, string]> = {
  draft: ["ยังไม่ส่งครัว", "Not sent"],
  queued: ["รอครัวรับงาน", "Kitchen pending"],
  preparing: ["กำลังทำ", "In progress"],
  ready: ["พร้อมเสิร์ฟ", "Ready to serve"],
  delivered: ["ส่งมอบแล้ว", "Delivered"],
  cancelled: ["ยกเลิกแล้ว", "Cancelled"],
};

function Tag({ value, kitchen = false }: { value: string; kitchen?: boolean }) {
  const language = useFrontLanguage();
  return (
    <span className={styles.tag}>
      {kitchen && kitchenLabels[value]
        ? kitchenLabels[value][language === "th" ? 0 : 1]
        : frontText(labels[value] ?? value, language)}
    </span>
  );
}

function FeatureWidget(props: ComponentProps<typeof FeatureWidgetBase>) {
  const language = useFrontLanguage();
  return <FeatureWidgetBase {...props} title={frontText(props.title, language)} />;
}

function Button({
  children,
  onClick,
  disabled,
  secondary = false,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  secondary?: boolean;
  type?: "button" | "submit";
}) {
  const language = useFrontLanguage();
  return (
    <UiButton
      variant="secondary"
      type={type}
      clientManaged={type === "submit"}
      onClick={onClick}
      disabled={disabled}
      className={`${styles.button} ${secondary ? styles.secondary : ""}`}
    >
      {typeof children === "string" ? frontText(children, language) : children}
    </UiButton>
  );
}

type Send = (
  op: string,
  input: unknown,
  select?: boolean,
) => Promise<Record<string, unknown> | undefined>;

function CatalogImageUpload({
  product,
  api,
  refresh,
}: {
  product: Row;
  api: string;
  refresh: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [retry, setRetry] = useState(false);
  const pending = useRef<{ file: File; key: string; version: number } | null>(null);
  async function upload(e: FormEvent<HTMLFormElement>) {
    const f = fields(e),
      file = pending.current?.file ?? f.get("image");
    if (!(file instanceof File) || !file.size) return;
    if (file.size > 10 * 1024 * 1024) {
      setError("ใช้ไฟล์ PNG, JPEG หรือ WebP ไม่เกิน 10 MB");
      return;
    }
    const command = pending.current ?? {
      file,
      key: crypto.randomUUID(),
      version: n(product, "catalog_version"),
    };
    pending.current = command;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const r = await workFetch(`${api}/images/upload`, {
        method: "POST",
        headers: {
          "Content-Type": command.file.type,
          "Idempotency-Key": command.key,
          "x-pos-product": s(product, "id"),
          "x-pos-catalog-version": String(command.version),
        },
        body: command.file,
      });
      const body = await r.json();
      if (!r.ok) {
        if (r.status < 500) pending.current = null;
        throw new Error(body.error?.message ?? "Upload failed");
      }
      pending.current = null;
      setNotice("บันทึกรูปสินค้าและ thumbnail แล้ว");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Upload failed");
    } finally {
      setBusy(false);
      setRetry(!!pending.current);
    }
  }
  return (
    <form className={styles.form} onSubmit={(e) => void upload(e)}>
      <Field>
        รูปสินค้า / Product image
        <TextInput
          name="image"
          type="file"
          accept="image/png,image/jpeg,image/webp"
          required={!retry}
          disabled={busy || retry}
        />
      </Field>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <Button type="submit" disabled={busy}>
        {retry ? "ตรวจผล upload เดิม / Retry upload" : "บันทึกรูปสินค้า / Save image"}
      </Button>
    </form>
  );
}

function Ingredients({
  products,
  value,
  onChange,
}: {
  products: Row[];
  value: Array<{
    productId: string;
    quantityMilli: number;
    unitLabel?: string;
    conversionMicros?: number;
  }>;
  onChange: (
    v: Array<{
      productId: string;
      quantityMilli: number;
      unitLabel?: string;
      conversionMicros?: number;
    }>,
  ) => void;
}) {
  return (
    <div className={styles.ingredients}>
      {value.map((v, i) => (
        <div key={i}>
          <Select
            aria-label={`Ingredient ${i + 1}`}
            value={v.productId}
            onChange={(e) =>
              onChange(value.map((p, j) => (j === i ? { ...p, productId: e.target.value } : p)))
            }
          >
            <option value="">Select ingredient</option>
            {products.map((p) => (
              <option key={s(p, "id")} value={s(p, "id")}>
                {s(p, "name")} ({s(p, "unit")})
              </option>
            ))}
          </Select>
          <TextInput
            aria-label={`Ingredient quantity ${i + 1}`}
            type="number"
            min="0.001"
            step="0.001"
            value={v.quantityMilli / 1000}
            onChange={(e) =>
              onChange(
                value.map((p, j) =>
                  j === i ? { ...p, quantityMilli: Math.round(Number(e.target.value) * 1000) } : p,
                ),
              )
            }
          />
          <TextInput
            aria-label={`Ingredient unit ${i + 1}`}
            placeholder="Entered unit"
            value={v.unitLabel ?? ""}
            onChange={(e) =>
              onChange(value.map((p, j) => (j === i ? { ...p, unitLabel: e.target.value } : p)))
            }
          />
          <Field>
            1 unit = base unit
            <TextInput
              aria-label={`Conversion rate ${i + 1}`}
              type="number"
              min="0.000001"
              step="0.000001"
              value={(v.conversionMicros ?? 1000000) / 1000000}
              onChange={(e) =>
                onChange(
                  value.map((p, j) =>
                    j === i
                      ? { ...p, conversionMicros: Math.round(Number(e.target.value) * 1000000) }
                      : p,
                  ),
                )
              }
            />
          </Field>
          <Button secondary onClick={() => onChange(value.filter((_, j) => i !== j))}>
            Remove
          </Button>
        </div>
      ))}
      <Button
        secondary
        onClick={() => onChange([...value, { productId: "", quantityMilli: 1000 }])}
      >
        Add ingredient
      </Button>
    </div>
  );
}

export function Catalog({
  data,
  busy,
  send,
  allowRecipes,
  api,
  refresh,
}: {
  data: PosWorkspace;
  busy: boolean;
  send: Send;
  allowRecipes: boolean;
  api: string;
  refresh: () => Promise<void>;
}) {
  const [productId, setProductId] = useState(""),
    [catalogKind, setCatalogKind] = useState("stock"),
    [recipe, setRecipe] = useState<
      Array<{
        productId: string;
        quantityMilli: number;
        unitLabel?: string;
        conversionMicros?: number;
      }>
    >([]),
    [modifiers, setModifiers] = useState<
      Array<{
        id: string;
        name: string;
        priceMinor: number;
        ingredients: Array<{
          productId: string;
          quantityMilli: number;
          unitLabel?: string;
          conversionMicros?: number;
        }>;
      }>
    >([]);
  const product = data.products.find((p) => p.id === productId);
  return (
    <FeatureWidget
      id="pos-products-menus-recipes"
      title={allowRecipes ? "Products, menus & recipes" : "Product scan details"}
      className={styles.panel}
    >
      <div className={styles.fields}>
        <Field>
          Product
          <Select
            value={productId}
            onChange={(e) => {
              const p = data.products.find((p) => p.id === e.target.value);
              setProductId(e.target.value);
              setCatalogKind(s(p ?? {}, "kind") || "stock");
              setRecipe(JSON.parse(s(p ?? {}, "recipe_json") || "[]"));
              setModifiers(JSON.parse(s(p ?? {}, "modifiers_json") || "[]"));
            }}
          >
            <option value="">Select product</option>
            {data.products.map((p) => (
              <option key={s(p, "id")} value={s(p, "id")}>
                {s(p, "name")}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {product?.kind === "recipe" && !allowRecipes ? (
        <p className={styles.muted}>
          Recipe details are available after a restaurant register is added.
        </p>
      ) : (
        product && (
          <form
            key={`${productId}:${product.catalog_version}`}
            className={`${styles.fields} ${styles.spaced}`}
            onSubmit={(e) => {
              const f = fields(e);
              void send("catalog.save", {
                productId,
                barcode: text(f, "barcode"),
                category: text(f, "category"),
                kind: text(f, "kind"),
                station: allowRecipes ? text(f, "station") : "Kitchen",
                taxInclusive: f.get("taxInclusive") === "on",
                enabled: f.get("enabled") === "on",
                registerIds: f.getAll("register").map(String),
                recipe,
                modifiers,
                expectedVersion: product.catalog_version,
              });
            }}
          >
            <Field orientation="inline" className={styles.check}>
              <TextInput name="enabled" type="checkbox" defaultChecked={Boolean(product.enabled)} />
              เปิดขายใน POS / Enabled for sale
            </Field>
            <fieldset>
              <legend>จุดขายที่เปิดขาย / Register catalog</legend>
              {data.registers.map((r) => (
                <Field orientation="inline" key={s(r, "id")} className={styles.check}>
                  <TextInput
                    type="checkbox"
                    name="register"
                    value={s(r, "id")}
                    defaultChecked={JSON.parse(s(product, "register_ids_json") || "[]").includes(
                      r.id,
                    )}
                  />
                  {s(r, "name")}
                </Field>
              ))}
            </fieldset>
            <Field>
              Barcode
              <TextInput name="barcode" defaultValue={s(product, "barcode")} />
            </Field>
            <Field>
              Category
              <TextInput name="category" defaultValue={s(product, "category")} />
            </Field>
            <Field>
              Type
              <Select
                name="kind"
                value={catalogKind}
                onChange={(event) => setCatalogKind(event.target.value)}
              >
                <option value="stock">Stock item</option>
                {allowRecipes && <option value="recipe">Recipe item</option>}
                <option value="service">Service</option>
              </Select>
            </Field>
            {allowRecipes && catalogKind === "recipe" ? (
              <Field>
                Kitchen station
                <TextInput name="station" defaultValue={s(product, "station")} required />
              </Field>
            ) : (
              <TextInput type="hidden" name="station" value={s(product, "station") || "Kitchen"} />
            )}
            <Field orientation="inline" className={styles.check}>
              <TextInput
                name="taxInclusive"
                type="checkbox"
                defaultChecked={Boolean(product.tax_inclusive)}
              />
              Product price includes tax
            </Field>
            {allowRecipes && (
              <>
                {catalogKind === "recipe" && (
                  <div>
                    <strong>Ingredients per menu unit</strong>
                    <p className={styles.muted}>Base-unit quantity, e.g. 0.2 kg = 200 g</p>
                    <Ingredients products={data.products} value={recipe} onChange={setRecipe} />
                  </div>
                )}
                <div>
                  <strong>{catalogKind === "recipe" ? "Menu modifiers" : "Product options"}</strong>
                  {modifiers.map((m, i) => (
                    <div key={m.id} className={styles.panel}>
                      <Field>
                        Name
                        <TextInput
                          value={m.name}
                          onChange={(e) =>
                            setModifiers(
                              modifiers.map((v, j) =>
                                j === i ? { ...v, name: e.target.value } : v,
                              ),
                            )
                          }
                        />
                      </Field>
                      <Field>
                        Additional price
                        <TextInput
                          type="number"
                          min="0"
                          step="0.01"
                          value={m.priceMinor / 100}
                          onChange={(e) =>
                            setModifiers(
                              modifiers.map((v, j) =>
                                j === i
                                  ? { ...v, priceMinor: Math.round(Number(e.target.value) * 100) }
                                  : v,
                              ),
                            )
                          }
                        />
                      </Field>
                      {(catalogKind === "recipe" || m.ingredients.length > 0) && (
                        <>
                          {catalogKind !== "recipe" && (
                            <p className={styles.muted}>
                              Ingredient options require a recipe item. Remove ingredients or choose
                              Recipe item before saving.
                            </p>
                          )}
                          <Ingredients
                            products={data.products}
                            value={m.ingredients}
                            onChange={(ingredients) =>
                              setModifiers(
                                modifiers.map((v, j) => (j === i ? { ...v, ingredients } : v)),
                              )
                            }
                          />
                        </>
                      )}
                      <Button
                        secondary
                        onClick={() => setModifiers(modifiers.filter((_, j) => j !== i))}
                      >
                        Remove modifier
                      </Button>
                    </div>
                  ))}
                  <Button
                    secondary
                    onClick={() =>
                      setModifiers([
                        ...modifiers,
                        { id: crypto.randomUUID(), name: "", priceMinor: 0, ingredients: [] },
                      ])
                    }
                  >
                    Add modifier
                  </Button>
                </div>
              </>
            )}
            <Button type="submit" disabled={busy}>
              Save POS product
            </Button>
          </form>
        )
      )}
      {product && n(product, "catalog_version") > 0 && (
        <CatalogImageUpload key={productId} product={product} api={api} refresh={refresh} />
      )}
    </FeatureWidget>
  );
}

function RetailSetup({
  data,
  base,
  busy,
  send,
}: {
  data: PosWorkspace;
  base: string;
  busy: boolean;
  send: Send;
}) {
  const [mode, setMode] = useState<"retail" | "restaurant">("retail"),
    [registerName, setRegisterName] = useState("");
  const activeRetailRegisters = data.registers.filter(
    (row) => s(row, "mode") === mode && Boolean(row.active),
  );
  const existingRetailRegisters = data.registers.filter((row) => s(row, "mode") === mode);
  const [registerId, setRegisterId] = useState(
      activeRetailRegisters.length === 1 ? s(activeRetailRegisters[0], "id") : "",
    ),
    [warehouseId, setWarehouseId] = useState(""),
    [bankAccountId, setBankAccountId] = useState("");
  const commerce = data.commerce,
    chartId = s(commerce ?? {}, "chart_id"),
    currency = s(commerce ?? {}, "currency");
  const chart = data.charts.find((row) => s(row, "id") === chartId);
  const warehouses = data.warehouses;
  const selectedRegister = activeRetailRegisters.find((row) => s(row, "id") === registerId);
  const selectedWarehouse = selectedRegister
    ? warehouses.find((row) => s(row, "id") === s(selectedRegister, "warehouse_id"))
    : (warehouses.find((row) => s(row, "id") === warehouseId) ??
      (warehouses.length === 1 ? warehouses[0] : undefined));
  const bankAccounts = data.bankAccounts.filter(
    (row) =>
      s(row, "chart_id") === chartId && (!s(row, "currency") || s(row, "currency") === currency),
  );
  const commerceAccounts = posCommerceAccountDefaults.map((source) => {
    const accountId = s(commerce ?? {}, source.field);
    const account = data.accounts.find(
      (row) =>
        s(row, "id") === accountId &&
        s(row, "chart_id") === chartId &&
        s(row, "category") === source.category &&
        (!s(row, "currency") || s(row, "currency") === currency),
    );
    return { source, account };
  });
  const issues: string[] = [];
  if (!commerce) issues.push("Set up Commerce and Finance before creating POS settings.");
  else {
    if (!chart || s(chart, "status") !== "active")
      issues.push("Select an active chart of accounts in Commerce and Finance.");
    if (chart && s(chart, "base_currency") !== currency)
      issues.push(
        `Commerce uses ${currency}, but the chart base currency is ${s(chart, "base_currency")}. Align them in Finance before continuing.`,
      );
    if (commerceAccounts.some((item) => !item.account))
      issues.push(
        "One or more Commerce accounts are unavailable or use a different currency. Review Commerce and Finance settings.",
      );
    if (!activeRetailRegisters.length && existingRetailRegisters.length)
      issues.push(
        "The existing retail register is inactive. Add an active retail register in Advanced POS settings before setup; the inactive register will remain unchanged.",
      );
    if (activeRetailRegisters.length > 1 && !selectedRegister)
      issues.push("Choose which active retail register will use the POS settings.");
    if (selectedRegister && !selectedWarehouse)
      issues.push("The chosen retail register must use an active warehouse in its branch.");
    if (!activeRetailRegisters.length && !existingRetailRegisters.length && !warehouses.length)
      issues.push("Add an active warehouse in Commerce before creating a register.");
    if (
      !activeRetailRegisters.length &&
      !existingRetailRegisters.length &&
      warehouses.length > 1 &&
      !warehouses.some((row) => s(row, "id") === warehouseId)
    )
      issues.push("Choose which active warehouse the retail register will use.");
    if (bankAccounts.length > 1 && !bankAccounts.some((row) => s(row, "id") === bankAccountId))
      issues.push("Choose which bank account will receive POS transfer payments.");
  }
  const canBootstrap = Boolean(
    data.permissions["pos.accounting.prepare"] && data.permissions["finance.write"],
  );
  const chosenWarehouse = selectedWarehouse;
  const chosenBank =
    bankAccounts.find((row) => s(row, "id") === bankAccountId) ??
    (bankAccounts.length === 1 ? bankAccounts[0] : undefined);
  const generatedAccounts = posGeneratedAccountSpecs.filter(
    (spec) => spec.key !== "bank" || bankAccounts.length === 0,
  );
  const submit = () =>
    void send("settings.bootstrap", {
      mode,
      ...(registerName.trim() ? { registerName: registerName.trim() } : {}),
      ...(selectedRegister ? { registerId: s(selectedRegister, "id") } : {}),
      ...(!selectedRegister && chosenWarehouse ? { warehouseId: s(chosenWarehouse, "id") } : {}),
      ...(chosenBank ? { bankAccountId: s(chosenBank, "id") } : {}),
    });

  return (
    <section className={styles.setupCard} aria-labelledby="pos-retail-setup-title">
      <div className={styles.setupHeading}>
        <p className={styles.eyebrow}>
          First-time setup · {mode === "restaurant" ? "Restaurant" : "Retail"}
        </p>
        <h2 id="pos-retail-setup-title">
          Set up POS for {mode === "restaurant" ? "a restaurant" : "a retail store"}
        </h2>
        <p>
          POS will reuse your Commerce accounts and add the accounts listed below. You can open a
          cash session after setup.
        </p>
      </div>
      <Field className={styles.setupSelect}>
        Business mode
        <Select
          value={mode}
          onChange={(e) => {
            setMode(e.target.value as "retail" | "restaurant");
            setRegisterId("");
          }}
        >
          <option value="retail">Retail</option>
          <option value="restaurant">Restaurant</option>
        </Select>
      </Field>
      <Field className={styles.setupSelect}>
        New register name
        <TextInput
          value={registerName}
          placeholder={`Main ${mode} register`}
          onChange={(e) => setRegisterName(e.target.value)}
          maxLength={200}
        />
      </Field>
      {!commerce ? (
        <div className={styles.blocker}>
          <strong>Commerce setup is required first.</strong>
          <p>
            POS uses the company chart, currency, accounts, and active warehouse configured in
            Commerce and Finance.
          </p>
          <div className={styles.actions}>
            <UiActionLink variant="primary" className={styles.button} href={`${base}/pos/settings/startup`}>
              Prepare POS accounts and catalogue
            </UiActionLink>
            <UiActionLink variant="secondary" className={styles.button} href={canonicalUiHref(`${base}/commerce`)}>
              Open Commerce setup
            </UiActionLink>
            <UiActionLink
              variant="secondary"
              className={`${styles.button} ${styles.secondary}`}
              href={canonicalUiHref(`${base}/finance/setup`)}
            >
              Open Finance setup
            </UiActionLink>
          </div>
        </div>
      ) : (
        <>
          <dl className={styles.setupSummary}>
            <div>
              <dt>Chart of accounts</dt>
              <dd>
                {s(chart ?? {}, "name") || "Not available"}
                {chart && <> · {s(chart, "base_currency")}</>}
              </dd>
            </div>
            <div>
              <dt>Commerce currency</dt>
              <dd>{currency || "Not configured"}</dd>
            </div>
            <div>
              <dt>{selectedRegister ? "Existing retail register" : "Register warehouse"}</dt>
              <dd>
                {selectedRegister
                  ? `${s(selectedRegister, "name")} · ${s(chosenWarehouse ?? {}, "name") || "Warehouse unavailable"}`
                  : chosenWarehouse
                    ? s(chosenWarehouse, "name")
                    : "Choose an active warehouse"}
              </dd>
            </div>
            <div>
              <dt>POS transfer bank</dt>
              <dd>
                {chosenBank
                  ? `${s(chosenBank, "code")} · ${s(chosenBank, "name")}`
                  : bankAccounts.length
                    ? "Choose a bank account"
                    : "A POS bank account will be created"}
              </dd>
            </div>
          </dl>
          {activeRetailRegisters.length > 1 && (
            <Field className={styles.setupSelect}>
              Retail register to configure
              <Select value={registerId} onChange={(event) => setRegisterId(event.target.value)}>
                <option value="">Choose a retail register</option>
                {activeRetailRegisters.map((row) => (
                  <option key={s(row, "id")} value={s(row, "id")}>
                    {s(row, "name")} · {s(row, "warehouse_name")}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          {!activeRetailRegisters.length &&
            !existingRetailRegisters.length &&
            warehouses.length > 1 && (
              <Field className={styles.setupSelect}>
                Warehouse for the retail register
                <Select
                  value={warehouseId}
                  onChange={(event) => setWarehouseId(event.target.value)}
                >
                  <option value="">Choose a warehouse</option>
                  {warehouses.map((row) => (
                    <option key={s(row, "id")} value={s(row, "id")}>
                      {s(row, "name")}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
          {bankAccounts.length > 1 && (
            <Field className={styles.setupSelect}>
              Bank account for transfer receipts
              <Select
                value={bankAccountId}
                onChange={(event) => setBankAccountId(event.target.value)}
              >
                <option value="">Choose a bank account</option>
                {bankAccounts.map((row) => (
                  <option key={s(row, "id")} value={s(row, "id")}>
                    {s(row, "code")} · {s(row, "name")}
                    {s(row, "currency") ? ` · ${s(row, "currency")}` : ""}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <div className={styles.accountPlan}>
            <section className={styles.accountGroup} aria-labelledby="pos-commerce-accounts-title">
              <div className={styles.accountGroupHeading}>
                <div>
                  <p className={styles.planKicker}>Reuse from Commerce</p>
                  <h3 id="pos-commerce-accounts-title">Existing accounts</h3>
                </div>
                <span className={styles.planCount}>{commerceAccounts.length} accounts</span>
              </div>
              <ul>
                {commerceAccounts.map(({ source, account }) => (
                  <li key={source.key}>
                    <span>{source.label}</span>
                    <strong>
                      {account ? `${s(account, "code")} · ${s(account, "name")}` : "Unavailable"}
                    </strong>
                  </li>
                ))}
              </ul>
            </section>
            <section className={styles.accountGroup} aria-labelledby="pos-new-accounts-title">
              <div className={styles.accountGroupHeading}>
                <div>
                  <p className={styles.planKicker}>Add to this chart</p>
                  <h3 id="pos-new-accounts-title">POS accounts</h3>
                </div>
                <span className={styles.planCount}>{generatedAccounts.length} accounts</span>
              </div>
              <p className={styles.planNote}>
                If a code is already in use, setup adds a numeric suffix and leaves that account
                unchanged.
              </p>
              <ul>
                {generatedAccounts.map((spec) => (
                  <li key={spec.key}>
                    <span>{spec.name}</span>
                    <strong>
                      {spec.code} · {spec.category}
                      {spec.key === "bank" ? " · bank control" : ""}
                    </strong>
                  </li>
                ))}
              </ul>
            </section>
          </div>
        </>
      )}
      {issues.length > 0 && (
        <div className={styles.blocker} role="alert">
          <strong>Resolve these items before setup</strong>
          <ul>
            {issues.map((issue) => (
              <li key={issue}>{issue}</li>
            ))}
          </ul>
          {commerce && (
            <div className={styles.actions}>
              <UiActionLink
                variant="secondary"
                className={`${styles.button} ${styles.secondary}`}
                href={canonicalUiHref(`${base}/commerce`)}
              >
                Review Commerce
              </UiActionLink>
              <UiActionLink
                variant="secondary"
                className={`${styles.button} ${styles.secondary}`}
                href={canonicalUiHref(`${base}/finance/setup`)}
              >
                Review Finance
              </UiActionLink>
            </div>
          )}
        </div>
      )}
      {!canBootstrap && (
        <p className={styles.muted}>
          A user with both POS configure and Finance write access must run this setup.
        </p>
      )}
      <div className={styles.setupActions}>
        <Button onClick={submit} disabled={busy || issues.length > 0 || !canBootstrap}>
          Set up POS for {mode}
        </Button>
        <span className={styles.muted}>
          {selectedRegister
            ? "Adds POS settings to the existing register without changing it."
            : "Creates POS settings and one register in a single save."}{" "}
          It does not open a cash session.
        </span>
      </div>
    </section>
  );
}

export function Settings({
  data,
  base,
  busy,
  send,
  dailyPosRoot,
  storeMode = true,
  refresh,
}: {
  data: PosWorkspace;
  base: string;
  busy: boolean;
  send: Send;
  dailyPosRoot: string;
  storeMode?: boolean;
  refresh: () => Promise<void>;
}) {
  const fallback = data.commerce ?? {},
    stored = data.settings,
    accounts = stored ? (JSON.parse(s(stored, "accounts_json")) as PosAccounts) : null;
  const savedChartId = accounts?.chartId ?? s(fallback, "chart_id");
  const initialMappings = Object.fromEntries(
    accountKeys.map((key) => [key, accounts?.[key] ?? ""]),
  );
  const [chartId, setChartId] = useState(savedChartId);
  const [mappingValues, setMappingValues] = useState<Record<string, string>>(initialMappings);
  const [accountingOpen, setAccountingOpen] = useState(false),
    [showMappingErrors, setShowMappingErrors] = useState(false);
  const [enabled, setEnabled] = useState(Boolean(stored?.enabled)),
    [discount, setDiscount] = useState(n(stored ?? {}, "cashier_discount_bps") / 100);
  const accountNames: Record<string, string> = {
    cash: "Cash",
    bank: "Bank deposits",
    edc: "EDC clearing",
    stripe: "Stripe clearing",
    receivable: "POS receivables",
    revenue: "Revenue",
    outputTax: "Output tax",
    inventory: "Inventory",
    cogs: "Cost of sales",
    advance: "Advances received",
    wip: "Work in progress",
    waste: "Waste",
    refund: "Customer refunds payable",
    fees: "Fees and losses",
    cashDifference: "Cash variance",
    cashTransfer: "Cash drawer transfers",
    disputes: "Disputes pending",
  };
  const mappingGroups: { title: string; keys: (typeof accountKeys)[number][] }[] = [
    { title: "Payments and clearing", keys: ["cash", "bank", "edc", "stripe", "receivable"] },
    { title: "Sales and liabilities", keys: ["revenue", "outputTax", "advance", "disputes"] },
    { title: "Inventory and fulfilment", keys: ["inventory", "cogs", "wip", "waste"] },
    {
      title: "Refunds and adjustments",
      keys: ["refund", "fees", "cashDifference", "cashTransfer"],
    },
  ];
  const chart = data.charts.find((row) => s(row, "id") === chartId),
    chartChanged = Boolean(chartId && chartId !== savedChartId);
  const isMappingValid = (key: string, value = mappingValues[key] ?? "") =>
    Boolean(
      chartId &&
      value &&
      data.accounts.some((row) => s(row, "id") === value && s(row, "chart_id") === chartId),
    );
  const missingMappings = accountKeys.filter((key) => !isMappingValid(key));
  const activeRegisters = data.registers.filter((row) => Boolean(row.active));
  const barcodedProducts = data.products.filter((row) => s(row, "barcode").trim()).length;
  const accountingSummary = chart
    ? `${s(chart, "name")} · ${accountKeys.length - missingMappings.length}/${accountKeys.length} mappings ready`
    : "Select a chart of accounts";
  const registerSummary = data.registers.length
    ? `${activeRegisters.length} active · ${data.registers.length} total`
    : "No registers configured";
  const productSummary = data.products.length
    ? `${data.products.length} products · ${barcodedProducts} with barcode`
    : "No active products";
  const canConfigure = storeMode && Boolean(data.permissions["pos.configure"]), canPrepare = Boolean(data.permissions["pos.accounting.prepare"]), canApprove = Boolean(data.permissions["pos.accounting.approve"]);
  const handleChartChange = (nextChartId: string) => {
    setChartId(nextChartId);
    setMappingValues(
      nextChartId === savedChartId
        ? initialMappings
        : Object.fromEntries(accountKeys.map((key) => [key, ""])),
    );
    setShowMappingErrors(false);
    if (nextChartId !== savedChartId) setAccountingOpen(true);
  };
  const saveSettings = (event: FormEvent<HTMLFormElement>) => {
    const f = fields(event),
      nextChartId = text(f, "chartId");
    const nextMappings = Object.fromEntries(accountKeys.map((key) => [key, text(f, key)]));
    if (!canPrepare) {
      void send("settings.operations", { enabled:f.get("enabled")==="on", cashierDiscountBps:Math.round(Number(f.get("discount"))*100), expectedVersion:stored?.version??0 });
      return;
    }
    const missing = accountKeys.filter(
      (key) =>
        !nextChartId ||
        !text(f, key) ||
        !data.accounts.some(
          (row) => s(row, "id") === text(f, key) && s(row, "chart_id") === nextChartId,
        ),
    );
    if (missing.length) {
      setShowMappingErrors(true);
      setAccountingOpen(true);
      return;
    }
    void send("accounting.prepare", {
      enabled: f.get("enabled") === "on",
      currency: text(f, "currency"),
      timezone: "Asia/Bangkok",
      cashierDiscountBps: Math.round(Number(f.get("discount")) * 100),
      expectedVersion: stored?.version ?? 0,
      accounts: { ...nextMappings, chartId: nextChartId },
    });
  };
  const setupContent = stored ? (
    <section className={styles.setupCard} aria-labelledby="pos-setup-status">
      <div>
        <p className={styles.eyebrow}>POS setup</p>
        <h2 id="pos-setup-status">{stored.enabled ? "POS is ready" : "POS is paused"}</h2>
        <p>
          {stored.enabled
            ? "Registers and accounting policy are configured. Open a cash session with the opening float when the location is ready to trade."
            : "POS settings are saved, but new sales are paused."}
        </p>
      </div>
      <div className={styles.actions}>
        <UiActionLink
          variant="primary"
          className={styles.button}
          href={`${dailyPosRoot}?tab=shifts`}
        >
          Open a cash session
        </UiActionLink>
        <UiActionLink
          variant="secondary"
          className={`${styles.button} ${styles.secondary}`}
          href={`${dailyPosRoot}?tab=sale`}
        >
          Go to sales
        </UiActionLink>
      </div>
      {data.products.length === 0 && (
        <p className={styles.notice}>
          Add products in <Link href={canonicalUiHref(`${base}/commerce`)}>Commerce</Link> before the first sale.
        </p>
      )}
    </section>
  ) : canPrepare ? (
    <RetailSetup data={data} base={base} busy={busy} send={send} />
  ) : (
    <section className={styles.setupCard}>
      <div>
        <p className={styles.eyebrow}>POS setup</p>
        <h2>POS is not configured</h2>
        <p>A POS administrator must complete the first-time setup.</p>
      </div>
    </section>
  );
  if (!canConfigure && !canPrepare && !canApprove) return setupContent;
  return (
    <>
      {setupContent}
      <FeatureWidget id="pos-sales-settings" title="Sales settings" className={styles.panel}>
        <form className={styles.settingsForm} onSubmit={saveSettings}>
          {canConfigure ? <div className={styles.salesSettings}>
            <Field orientation="inline" className={styles.salesToggle}>
              <TextInput
                name="enabled"
                type="checkbox"
                checked={enabled}
                onChange={(event) => setEnabled(event.target.checked)}
              />
              <span>
                <strong>Accept new POS sales</strong>
                <small>Pause POS sales without changing the register or accounting setup.</small>
              </span>
            </Field>
            <Field>
              Maximum cashier discount (%)
              <TextInput
                name="discount"
                type="number"
                min="0"
                max="100"
                step="0.01"
                value={discount}
                onChange={(event) => setDiscount(Number(event.target.value))}
              />
            </Field>
          </div> : <><input type="hidden" name="enabled" value={enabled ? "on" : ""}/><input type="hidden" name="discount" value={discount}/></>}
          {(canPrepare || canApprove) && <details
            className={styles.settingsDisclosure}
            open={accountingOpen || chartChanged}
            onToggle={(event) => {
              if (!chartChanged) setAccountingOpen(event.currentTarget.open);
            }}
          >
            <summary>
              <span>Accounting</span>
              <span className={styles.disclosureSummaryValue}>{accountingSummary}</span>
            </summary>
            <div className={styles.disclosureBody}>
              <p className={styles.muted}>
                These mappings connect POS payments, sales, inventory and adjustments to the company
                chart.
              </p>
              {chartChanged && (
                <p className={styles.mappingNotice} role="status">
                  Chart changed. Select an account from this chart for each mapping before saving.
                </p>
              )}
              {showMappingErrors && missingMappings.length > 0 && (
                <p className={styles.mappingErrorSummary} role="alert">
                  Select accounts for: {missingMappings.map((key) => accountNames[key]).join(", ")}.
                </p>
              )}
              <div className={styles.accountingControls}>
                <Field>
                  Currency
                  <TextInput
                    name="currency"
                    defaultValue={s(stored ?? fallback, "currency") || "THB"}
                    pattern="[A-Z]{3}"
                    required
                  />
                </Field>
                <Field>
                  Chart of accounts
                  <Select
                    name="chartId"
                    value={chartId}
                    onChange={(event) => handleChartChange(event.target.value)}
                  >
                    <option value="">Select chart of accounts</option>
                    {data.charts.map((row) => (
                      <option key={s(row, "id")} value={s(row, "id")}>
                        {s(row, "name")} · {s(row, "base_currency")}
                      </option>
                    ))}
                  </Select>
                </Field>
              </div>
              {mappingGroups.map((group) => (
                <section
                  className={styles.accountMappingGroup}
                  key={group.title}
                  aria-label={group.title}
                >
                  <div className={styles.mappingGroupHeading}>
                    <h3>{group.title}</h3>
                    <span>{group.keys.length} mappings</span>
                  </div>
                  <div className={styles.accountMappingGrid}>
                    {group.keys.map((key) => {
                      const invalid = (showMappingErrors || chartChanged) && !isMappingValid(key);
                      const helpId = `pos-account-${key}-help`;
                      return (
                        <Field key={key} className={styles.mappingField}>
                          {accountNames[key]}
                          <Select
                            name={key}
                            value={mappingValues[key] ?? ""}
                            onChange={(event) =>
                              setMappingValues((current) => ({
                                ...current,
                                [key]: event.target.value,
                              }))
                            }
                            aria-invalid={invalid}
                            aria-describedby={helpId}
                          >
                            <option value="">Select account</option>
                            {data.accounts
                              .filter((row) => s(row, "chart_id") === chartId)
                              .map((row) => (
                                <option key={s(row, "id")} value={s(row, "id")}>
                                  {s(row, "code")} · {s(row, "name")}
                                </option>
                              ))}
                          </Select>
                          <span
                            id={helpId}
                            className={invalid ? styles.mappingFieldError : styles.mappingFieldHelp}
                          >
                            {invalid
                              ? "Select an account from the selected chart."
                              : "Account used for this POS posting."}
                          </span>
                        </Field>
                      );
                    })}
                  </div>
                </section>
              ))}
              {!data.accounts.some((row) => s(row, "chart_id") === chartId) && (
                <p className={styles.mappingNotice}>
                  No active accounts are available in this chart. Add or activate accounts in{" "}
                  <Link href={canonicalUiHref(`${base}/finance/accounting`)}>Finance</Link> first.
                </p>
              )}
            </div>
          </details>}
          <div className={styles.settingsActions}>
            <Button type="submit" disabled={busy || (!canPrepare && !canConfigure)}>
              {canPrepare ? "Prepare accounting mapping" : "Save store settings"}
            </Button>
            <span className={styles.muted}>
              {stored?.accounting_approved_by ? "Accounting mapping approved" : "Independent accounting approval required"}
            </span>
          </div>
        </form>
      </FeatureWidget>
      {canApprove && stored && <Button disabled={busy || Boolean(stored.accounting_approved_by)} onClick={() => void send("accounting.approve", {expectedVersion:stored.version})}>Approve accounting mapping</Button>}
      {canConfigure && <FormDisclosure className={styles.settingsDisclosure}>
        <summary>
          <span>Registers</span>
          <span className={styles.disclosureSummaryValue}>{registerSummary}</span>
        </summary>
        <div className={styles.disclosureBody}>
          {data.registers.length > 0 && (
            <ul className={styles.registerList}>
              {data.registers.map((row) => (
                <li key={s(row, "id")}>
                  <strong>{s(row, "name")}</strong>
                  <span>
                    {s(row, "mode") === "restaurant" ? "Restaurant" : "Retail"} ·{" "}
                    {s(row, "warehouse_name")}
                  </span>
                  <Tag value={row.active ? "active" : "inactive"} />
                </li>
              ))}
            </ul>
          )}
          <section className={styles.panel}>
            <h2>Add register</h2>
            <form
              className={styles.fields}
              onSubmit={(event) => {
                const f = fields(event);
                void send("register.save", {
                  name: text(f, "name"),
                  warehouseId: text(f, "warehouse"),
                  propertyId: text(f, "property") || null,
                  mode: text(f, "mode"),
                  active: true,
                  expectedVersion: 0,
                });
              }}
            >
              <Field>
                Register name
                <TextInput name="name" required />
              </Field>
              <Field>
                Warehouse
                <Select name="warehouse" required>
                  <option value="">Select a warehouse</option>
                  {data.warehouses.map((row) => (
                    <option key={s(row, "id")} value={s(row, "id")}>
                      {s(row, "name")}
                    </option>
                  ))}
                </Select>
              </Field>
              <Field>
                Register type
                <Select name="mode">
                  <option value="retail">Retail</option>
                  <option value="restaurant">Restaurant</option>
                </Select>
              </Field>
              <Field>
                Linked hotel (restaurant only)
                <Select name="property"><option value="">Independent shop</option>{data.hotelLinks.map(row=><option key={s(row,"id")} value={s(row,"id")}>{s(row,"name")}</option>)}</Select>
              </Field>
              <Button type="submit" disabled={busy || !data.warehouses.length}>
                Add register
              </Button>
            </form>
            {!data.warehouses.length && (
              <p className={styles.muted}>
                Add a warehouse in <Link href={canonicalUiHref(`${base}/commerce`)}>Commerce</Link> first.
              </p>
            )}
          </section>
          {data.registers.some((row) => row.mode === "restaurant") && (
            <section className={`${styles.panel} ${styles.spaced}`}>
              <h2>Restaurant tables</h2>
              <form
                className={styles.fields}
                onSubmit={(event) => {
                  const f = fields(event);
                  void send("table.create", {
                    registerId: text(f, "register"),
                    name: text(f, "name"),
                    seats: Number(f.get("seats")),
                  });
                }}
              >
                <Field>
                  Restaurant register
                  <Select name="register">
                    {data.registers
                      .filter((row) => row.mode === "restaurant")
                      .map((row) => (
                        <option key={s(row, "id")} value={s(row, "id")}>
                          {s(row, "name")}
                        </option>
                      ))}
                  </Select>
                </Field>
                <Field>
                  Table name
                  <TextInput name="name" required />
                </Field>
                <Field>
                  Seats
                  <TextInput type="number" name="seats" min="1" defaultValue="2" required />
                </Field>
                <Button type="submit" disabled={busy}>
                  Add table
                </Button>
              </form>
            </section>
          )}
        </div>
      </FormDisclosure>}
      {canConfigure && <details className={styles.settingsDisclosure}>
        <summary>
          <span>Products &amp; scanning</span>
          <span className={styles.disclosureSummaryValue}>{productSummary}</span>
        </summary>
        <div className={styles.disclosureBody}>
          <p className={styles.muted}>
            Products come from Commerce. Set barcodes, categories and restaurant menu details here.
          </p>
          {data.products.length ? (
            <Catalog
              data={data}
              busy={busy}
              send={send}
              allowRecipes={data.registers.some((row) => row.mode === "restaurant")}
              api={`/api/v1${base}/pos`}
              refresh={refresh}
            />
          ) : (
            <section className={styles.panel}>
              <h2>No active products</h2>
              <p>
                Add products in <Link href={canonicalUiHref(`${base}/commerce`)}>Commerce</Link> before configuring
                barcode details or a restaurant menu.
              </p>
            </section>
          )}
        </div>
      </details>}
    </>
  );
}
