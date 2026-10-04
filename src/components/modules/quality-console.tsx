"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type { qualityDetail, qualityWorkspace } from "@/server/quality";
import type { qualityAlerts } from "@/server/quality-alerts";
import styles from "./quality-console.module.css";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/attachments/attachment-panel";
import { FormLayout } from "@/components/ui/foundation";

export function QualityConsole({
  base,
  data,
  detail,
  alerts = [],
}: {
  base: string;
  data?: Awaited<ReturnType<typeof qualityWorkspace>>;
  detail?: Awaited<ReturnType<typeof qualityDetail>>;
  alerts?: Awaited<ReturnType<typeof qualityAlerts>>;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [uploaded, setUploaded] = useState<{ id: string; setId: string; name: string }[]>([]),
    [attachmentsV2, setAttachmentsV2] = useState<boolean | null>(null),
    [inspectionSet, setInspectionSet] = useState<AttachmentPanelChange | null>(null),
    [dispositionSet, setDispositionSet] = useState<AttachmentPanelChange | null>(null),
    [productSearch, setProductSearch] = useState(""),
    [specProduct, setSpecProduct] = useState(""),
    [stockSearch, setStockSearch] = useState(""),
    [stockTarget, setStockTarget] = useState(""),
    recordVersion = Number(detail?.hold.version ?? 0),
    retry = useRef<{ fingerprint: string; key: string } | null>(null);
  const products = data?.products ?? [],
    productNeedle = productSearch.trim().toLocaleLowerCase(),
    visibleProducts = products.filter(
      (product) =>
        !productNeedle ||
        `${String(product.sku)} ${String(product.name)}`
          .toLocaleLowerCase()
          .includes(productNeedle),
    );
  const stockChoices = data
      ? data.stock.map((stock, index) => ({
          stock,
          index,
          sku: String(data.products.find((product) => product.id === stock.product_id)?.sku ?? ""),
        }))
      : [],
    stockNeedle = stockSearch.trim().toLocaleLowerCase(),
    visibleStock = stockChoices.filter(
      ({ stock, sku }) =>
        !stockNeedle ||
        `${String(stock.warehouse_name)} ${sku} ${String(stock.product_name)}`
          .toLocaleLowerCase()
          .includes(stockNeedle),
    );
  async function send(path: string, input: unknown, navigate = true) {
    if (busy) return;
    setBusy(true);
    setError("");
    const fingerprint = JSON.stringify({ path, input });
    if (retry.current?.fingerprint !== fingerprint)
      retry.current = { fingerprint, key: crypto.randomUUID() };
    try {
      const response = await workFetch(`/api/v1${base}/quality${path}`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": retry.current.key },
          body: JSON.stringify(input),
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) retry.current = null;
        throw new Error(result.error?.message ?? "Command failed.");
      }
      retry.current = null;
      if (!detail && navigate) router.push(`${base}/quality/${result.data.id}`);
      else router.refresh();
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Retry the same action to recover its result.",
      );
    } finally {
      setBusy(false);
    }
  }
  async function uploadEvidence(file: File) {
    if (!detail || busy) return;
    if (
      !["image/jpeg", "image/png", "image/webp", "application/pdf", "text/csv"].includes(
        file.type,
      ) ||
      file.size < 1 ||
      file.size > 10 * 1024 * 1024
    )
      return setError("Attach a JPEG, PNG, WebP, PDF or CSV file up to 10 MB.");
    setBusy(true);
    setError("");
    try {
      const reserve = await workFetch(`/api/v1${base}/uploads`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
          body: JSON.stringify({
            ownerType: "quality_hold",
            ownerId: detail.hold.id,
            expectedVersion: recordVersion,
            purpose: "quality_inspection_evidence",
            filename: file.name,
            mediaType: file.type,
            bytes: file.size,
          }),
        }),
        prepared = await reserve.json();
      if (!reserve.ok)
        throw new Error(prepared.error?.message ?? "Unable to reserve inspection evidence.");
      const stored = await workFetch(prepared.data.uploadUrl, {
          method: "PUT",
          headers: { "Content-Type": file.type },
          body: file,
        }),
        result = await stored.json();
      if (!stored.ok)
        throw new Error(result.error?.message ?? "Unable to store inspection evidence.");
      setUploaded((items) => [
        ...items,
        {
          id: String(result.data.id),
          setId: String(result.data.attachmentSetId ?? prepared.data.attachmentSetId),
          name: file.name,
        },
      ]);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Retry this evidence upload.");
    } finally {
      setBusy(false);
    }
  }
  const alertsPanel = alerts.length > 0 && (
    <section className={styles.alerts}>
      <h2>Quality escalations</h2>
      {alerts.map((alert) => (
        <article key={String(alert.id)}>
          <h3>
            {alert.phase === "overdue" ? "Overdue quality hold" : "Quality hold due soon"} ·{" "}
            {String(alert.product_name)}
          </h3>
          <p>
            {String(alert.warehouse_name)} · due {String(alert.due_date)} · {String(alert.status)}
          </p>
          {alert.acknowledged_by ? (
            <p>
              Acknowledged by {String(alert.acknowledged_by)}: {String(alert.reason)}
            </p>
          ) : alert.can_ack ? (
            <FormLayout
              className="ui-form-grid"
              onSubmit={(e) => {
                e.preventDefault();
                void send(
                  `/${alert.id}/acknowledge`,
                  {
                    expectedVersion: Number(alert.version),
                    reason: new FormData(e.currentTarget).get("reason"),
                  },
                  false,
                );
              }}
            >
              <Field appearance="standard">
                Follow-up evidence
                <TextArea appearance="standard"
                  name="reason"
                  minLength={3}
                  maxLength={2000}
                  required
                  placeholder="Who will inspect or release this hold, and by when?"
                />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy} className="primary">
                Acknowledge escalation
              </Button>
            </FormLayout>
          ) : (
            <p>Waiting for an eligible independent Quality recipient.</p>
          )}
        </article>
      ))}
    </section>
  );
  return (
    <section className={`card ${styles.console}`}>
      {error && (
        <InlineNotice tone="danger" role="alert" className={styles.error}>
          {error}
        </InlineNotice>
      )}

      {!detail?.canDisposition && alertsPanel}
      {data?.canManageSpecs && (
        <>
          <form
            className={`business-form ${styles.qualityForm} ${styles.specForm}`}
            onSubmit={(e) => {
              e.preventDefault();
              const f = new FormData(e.currentTarget),
                lines = String(f.get("metrics"))
                  .split("\n")
                  .map((x) => x.trim())
                  .filter(Boolean),
                metrics = lines.map((line) => {
                  const [code, label, unit, minimum = "", maximum = ""] = line
                    .split("|")
                    .map((x) => x.trim());
                  return { code, label, unit, minimum, maximum };
                });
              if (metrics.some((m) => !m.code || !m.label || !m.unit || (!m.minimum && !m.maximum)))
                return setError(
                  "Use code | label | unit | minimum | maximum for every measurement.",
                );
              void send(
                "/specifications",
                { productId: f.get("productId"), name: f.get("name"), metrics },
                false,
              );
            }}
          >
            <header className={styles.formHeading}>
              <div>
                <span className={styles.eyebrow}>QUALITY SETUP</span>
                <h2>Prepare product inspection specification</h2>
                <p>
                  A different Company Quality manager must activate the measurable acceptance limits
                  before new holds use them.
                </p>
              </div>
            </header>
            <div className={`${styles.fields} ${styles.specFields}`}>
              <div className={styles.fieldGroup}>
                <Field appearance="standard" className={styles.field}>
                  Find a product
                  <TextInput appearance="standard"
                    type="search"
                    autoComplete="off"
                    maxLength={120}
                    placeholder="Search by code or name"
                    value={productSearch}
                    onChange={(event) => {
                      setProductSearch(event.target.value);
                      setSpecProduct("");
                    }}
                  />
                </Field>
                <span className={styles.resultCount} aria-live="polite">
                  {productSearch
                    ? `${visibleProducts.length} matches`
                    : `${products.length} products available`}
                </span>
                <Field appearance="standard" className={styles.field}>
                  Product
                  <Select appearance="standard"
                    name="productId"
                    required
                    value={specProduct}
                    onChange={(event) => setSpecProduct(event.target.value)}
                  >
                    <option value="">Choose product</option>
                    {visibleProducts.length ? (
                      visibleProducts.map((product) => (
                        <option key={String(product.id)} value={String(product.id)}>
                          {String(product.sku)} · {String(product.name)}
                        </option>
                      ))
                    ) : (
                      <option value="" disabled>
                        No matching products
                      </option>
                    )}
                  </Select>
                </Field>
              </div>
              <Field appearance="standard" className={styles.field}>
                Specification name
                <TextInput appearance="standard" name="name" minLength={3} maxLength={160} required />
              </Field>
              <Field appearance="standard" className={`${styles.field} ${styles.spanAll}`}>
                Measurements
                <TextArea appearance="standard"
                  name="metrics"
                  required
                  placeholder={
                    "temperature | Storage temperature | °C | 2 | 8\nweight | Net weight | kg | 9.9 | 10.1"
                  }
                />
              </Field>
            </div>
            <p className={styles.helper}>
              One measurement per line: code | label | unit | minimum | maximum. Leave one limit
              blank for a one-sided check.
            </p>
            <div className={styles.actions}>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Save draft specification
              </Button>
            </div>
          </form>
          {data.specifications.length > 0 && (
            <section className={styles.specList}>
              <div className={styles.sectionHeading}>
                <h2>Inspection specifications</h2>
                <span>{data.specifications.length} versions</span>
              </div>
              <div className={styles.specGrid}>
                {data.specifications.map((spec) => (
                  <article className={styles.specItem} key={String(spec.id)}>
                    <h3>
                      {String(spec.product_name)} · {String(spec.name)}
                    </h3>
                    <p>
                      <span className={styles.status}>{String(spec.status)}</span> · version{" "}
                      {String(spec.version)}
                      {data.specMetrics
                        .filter((m) => m.spec_id === spec.id)
                        .map(
                          (m) =>
                            `${String(m.label)} ${m.minimum_value ?? "–"}–${m.maximum_value ?? "–"} ${String(m.unit)}`,
                        ).length
                        ? ` · ${data.specMetrics
                            .filter((m) => m.spec_id === spec.id)
                            .map(
                              (m) =>
                                `${String(m.label)} ${m.minimum_value ?? "–"}–${m.maximum_value ?? "–"} ${String(m.unit)}`,
                            )
                            .join("; ")}`
                        : ""}
                    </p>
                    {spec.status === "draft" && (
                      <FormLayout
                        className="ui-form-grid"
                        onSubmit={(e) => {
                          e.preventDefault();
                          const f = new FormData(e.currentTarget);
                          void send(
                            `/specifications/${spec.id}`,
                            {
                              expectedVersion: Number(spec.version),
                              action: f.get("action"),
                              reason: f.get("reason"),
                            },
                            false,
                          );
                        }}
                      >
                        <Field appearance="standard">
                          Decision
                          <Select appearance="standard" name="action">
                            <option value="activate">Activate for new holds</option>
                            <option value="cancel">Cancel draft</option>
                          </Select>
                        </Field>
                        <Field appearance="standard">
                          Independent review reason
                          <TextArea appearance="standard" name="reason" minLength={5} required />
                        </Field>
                        <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                          Record specification decision
                        </Button>
                      </FormLayout>
                    )}
                  </article>
                ))}
              </div>
            </section>
          )}
        </>
      )}
      {data?.canHold && (
        <form
          className={`business-form ${styles.qualityForm} ${styles.holdForm}`}
          onSubmit={(e) => {
            e.preventDefault();
            const form = new FormData(e.currentTarget),
              target = String(form.get("target")),
              lot = target.startsWith("lot:")
                ? data.lots.find((item) => item.id === target.slice(4))
                : null,
              stock = lot
                ? data.stock.find(
                    (item) =>
                      item.warehouse_id === lot.warehouse_id && item.product_id === lot.product_id,
                  )
                : data.stock[Number(target.slice(6))];
            if (!stock) return setError("Select stock to quarantine.");
            void send("", {
              warehouseId: stock.warehouse_id,
              productId: stock.product_id,
              expectedStockVersion: Number(stock.version),
              ...(lot ? { lotId: lot.id, expectedLotVersion: Number(lot.balance_version) } : {}),
              dueDate: form.get("dueDate"),
              reason: form.get("reason"),
            });
          }}
        >
          <header className={styles.formHeading}>
            <div>
              <span className={styles.eyebrow}>NEW HOLD</span>
              <h2>Quarantine stock</h2>
              <p>
                Choose the affected stock and explain why it needs review. Existing reservations
                will remain blocked until the hold is resolved.
              </p>
            </div>
          </header>
          <div className={`${styles.fields} ${styles.holdFields}`}>
            <div className={styles.stockPicker}>
              <Field appearance="standard" className={styles.field}>
                Find stock
                <TextInput appearance="standard"
                  type="search"
                  autoComplete="off"
                  maxLength={120}
                  placeholder="Search warehouse, product code or name"
                  value={stockSearch}
                  onChange={(event) => {
                    setStockSearch(event.target.value);
                    setStockTarget("");
                  }}
                />
              </Field>
              <Field appearance="standard" className={styles.field}>
                Product, lot / serial, and warehouse
                <Select appearance="standard"
                  name="target"
                  required
                  value={stockTarget}
                  onChange={(event) => setStockTarget(event.target.value)}
                >
                  <option value="">Select stock</option>
                  {visibleStock.length ? (
                    visibleStock.map(({ stock, index, sku }) => (
                      <optgroup
                        key={`${stock.warehouse_id}:${stock.product_id}`}
                        label={`${String(stock.warehouse_name)} · ${sku} · ${String(stock.product_name)}`}
                      >
                        <option value={`stock:${index}`}>
                          Entire product · {Number(stock.quantity_milli) / 1000} on hand
                        </option>
                        {data.lots
                          .filter(
                            (lot) =>
                              lot.warehouse_id === stock.warehouse_id &&
                              lot.product_id === stock.product_id,
                          )
                          .map((lot) => (
                            <option
                              key={String(lot.id)}
                              value={`lot:${lot.id}`}
                              disabled={Boolean(Number(lot.quality_held))}
                            >
                              {lot.serial_number
                                ? `Serial ${String(lot.serial_number)}`
                                : `Lot ${String(lot.lot_code)}`}{" "}
                              · {Number(lot.quantity_milli) / 1000}
                              {Number(lot.quality_held) ? " · already quarantined" : ""}
                            </option>
                          ))}
                      </optgroup>
                    ))
                  ) : (
                    <option value="" disabled>
                      No matching stock
                    </option>
                  )}
                </Select>
              </Field>
              <span className={styles.resultCount} aria-live="polite">
                {stockSearch
                  ? `${visibleStock.length} stock locations match`
                  : `${stockChoices.length} stock locations available`}
              </span>
            </div>
            <Field appearance="standard" className={styles.field}>
              Review due date
              <TextInput appearance="standard" type="date" name="dueDate" required />
            </Field>
            <Field appearance="standard" className={`${styles.field} ${styles.reasonField}`}>
              Reason
              <TextArea appearance="standard"
                name="reason"
                minLength={3}
                maxLength={2000}
                required
                placeholder="Describe the defect, risk, or evidence that requires inspection."
              />
            </Field>
          </div>
          <div className={styles.actions}>
            <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy} className="primary">
              Place quality hold
            </Button>
          </div>
        </form>
      )}
      {detail?.canDisposition && (
        <form
          className={`business-form ${styles.detailAction} ${styles.primaryAction}`}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`/${detail.hold.id}/disposition`, {
              expectedVersion: Number(detail.hold.version),
              attachmentSetIds:
                dispositionSet?.setId && dispositionSet.readyCount > 0 && dispositionSet.bindable
                  ? [dispositionSet.setId]
                  : [],
              dispositionDate: f.get("dispositionDate"),
              quantity: f.get("quantity"),
              reason: f.get("reason"),
            });
          }}
        >
          <span className={styles.eyebrow}>NEXT ACTION</span>
          <h2>Request controlled scrap</h2>
          <p>
            Finance will review and post the inventory and accounting adjustment. The stock stays
            quarantined until approval. Any quantity not included in this request remains
            quarantined in a linked hold.
          </p>
          <Field appearance="standard">
            Scrap date
            <TextInput appearance="standard" type="date" name="dispositionDate" required />
          </Field>
          <Field appearance="standard">
            Quantity to scrap · {String(detail.hold.product_unit ?? "units")}
            <TextInput appearance="standard"
              type="number"
              name="quantity"
              min="0.001"
              max={Number(detail.hold.held_quantity_milli) / 1000}
              defaultValue={Number(detail.hold.held_quantity_milli) / 1000}
              step="0.001"
              required
            />
          </Field>
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="quality_hold"
            targetId={String(detail.hold.id)}
            expectedTargetVersion={Number(detail.hold.version)}
            componentKind="action"
            componentRef="disposition"
            purposeKey="disposition_evidence"
            title="Disposition evidence"
            description="Attach destruction, scrap, return-to-supplier, or other controlled disposition records."
            visibility="Quality and authorized independent Finance reviewers can access these files."
            onChange={setDispositionSet}
          />
          <Field appearance="standard">
            Disposition evidence and reason
            <TextArea appearance="standard" name="reason" minLength={5} maxLength={2000} required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" className="primary">
            Send scrap request to Finance
          </Button>
        </form>
      )}
      {detail?.canDisposition && alertsPanel}
      {detail?.canInspect && (
        <form
          className={`business-form ${styles.detailAction}`}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget),
              measurements = detail.specMetrics.map((m) => ({
                metricId: String(m.id),
                value: String(f.get(`metric:${m.id}`)),
              })),
              attachmentSetIds = [
                ...new Set([
                  ...uploaded.map((file) => file.setId),
                  ...(inspectionSet?.setId && inspectionSet.readyCount > 0 && inspectionSet.bindable
                    ? [inspectionSet.setId]
                    : []),
                ]),
              ];
            void send(`/${detail.hold.id}/inspect`, {
              expectedVersion: recordVersion,
              ...(detail.specMetrics.length ? { measurements } : { result: f.get("result") }),
              attachmentIds: [],
              attachmentSetIds,
              evidence: f.get("evidence"),
            });
          }}
        >
          <h2>Record inspection</h2>
          {detail.hold.last_result === "fail" && (
            <p>
              The latest independent inspection failed. Record another inspection only after the
              stock has been checked again.
            </p>
          )}
          {detail.specMetrics.length ? (
            <>
              {detail.specMetrics.map((m) => (
                <Field appearance="standard" key={String(m.id)}>
                  {String(m.label)} ({String(m.unit)}) · accepted {m.minimum_value ?? "–"} to{" "}
                  {m.maximum_value ?? "–"}
                  <TextInput appearance="standard" type="number" step="any" name={`metric:${m.id}`} required />
                </Field>
              ))}
              <p>
                The system derives pass or fail from the activated specification captured by this
                hold.
              </p>
            </>
          ) : (
            <Field appearance="standard">
              Result
              <Select appearance="standard" name="result">
                <option value="fail">Fail — keep quarantined</option>
                <option value="pass">Pass — request independent release</option>
              </Select>
            </Field>
          )}
          {attachmentsV2 !== true && (
            <>
              <Field appearance="standard">
                Inspection files
                <TextInput appearance="standard"
                  type="file"
                  accept="image/jpeg,image/png,image/webp,application/pdf,text/csv"
                  disabled={busy || uploaded.length >= 10}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) void uploadEvidence(file);
                    event.currentTarget.value = "";
                  }}
                />
              </Field>
              {uploaded.map((file) => (
                <p key={file.id}>Ready to bind: {file.name}</p>
              ))}
            </>
          )}
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="quality_hold"
            targetId={String(detail.hold.id)}
            expectedTargetVersion={recordVersion}
            componentKind="action"
            componentRef="inspection"
            purposeKey="quality_inspection_evidence"
            title="Inspection files"
            description="Attach defect photos, measurement records, certificates, or the inspection report."
            visibility="Quality inspectors and authorized independent release reviewers can access these files."
            onAvailabilityChange={setAttachmentsV2}
            onChange={setInspectionSet}
          />
          <Field appearance="standard">
            Inspection evidence
            <TextArea appearance="standard" name="evidence" minLength={3} maxLength={2000} required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy} className="primary">
            Record inspection
          </Button>
        </form>
      )}
      {detail?.canRelease && (
        <form
          className={`business-form ${styles.detailAction}`}
          onSubmit={(e) => {
            e.preventDefault();
            void send(`/${detail.hold.id}/release`, {
              expectedVersion: Number(detail.hold.version),
              reason: new FormData(e.currentTarget).get("reason"),
            });
          }}
        >
          <h2>Approve release</h2>
          <p>
            Review the latest passing inspection. Any other active hold on this stock will remain in
            force.
          </p>
          <Field appearance="standard">
            Release reason
            <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" className="primary">
            Release this hold
          </Button>
        </form>
      )}

      {detail?.canCancel && (
        <form
          className={`business-form ${styles.detailAction}`}
          onSubmit={(e) => {
            e.preventDefault();
            void send(`/${detail.hold.id}/cancel`, {
              action: "request",
              expectedVersion: Number(detail.hold.version),
              reason: new FormData(e.currentTarget).get("reason"),
            });
          }}
        >
          <h2>Request cancellation</h2>
          <p>
            Use only for an uninspected hold created in error. Stock remains quarantined until
            another Quality manager approves. An inspected hold must follow quality disposition.
          </p>
          <Field appearance="standard">
            Why was this hold incorrect?
            <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary">
            Request independent cancellation
          </Button>
        </form>
      )}
      {detail?.canReviewCancellation && (
        <form
          className={`business-form ${styles.detailAction}`}
          onSubmit={(e) => {
            e.preventDefault();
            const f = new FormData(e.currentTarget);
            void send(`/${detail.hold.id}/cancel`, {
              action: f.get("action"),
              expectedVersion: Number(detail.hold.version),
              reason: f.get("reason"),
            });
          }}
        >
          <h2>Review cancellation</h2>
          <p>{String(detail.hold.cancel_reason)}</p>
          <p>Approval removes this hold only; other holds and reservations remain in force.</p>
          <Field appearance="standard">
            Decision
            <Select appearance="standard" name="action">
              <option value="reject">Reject — retain quarantine</option>
              <option value="approve">Approve cancellation of incorrect hold</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Independent evidence and reason
            <TextArea appearance="standard" name="reason" minLength={3} maxLength={2000} required />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary">
            Record cancellation decision
          </Button>
        </form>
      )}
      {detail?.hold.disposition_count_id && (
        <section>
          <h2>Disposition handoff</h2>
          <p>
            Status: {String(detail.hold.status)} ·{" "}
            <a href={`${base}/stock-counts/${detail.hold.disposition_count_id}`}>
              Open controlled stock and accounting adjustment
            </a>
          </p>
          {detail.dispositions.map((event) => (
            <p key={String(event.id)}>
              {String(event.action)} · {String(event.actor_id)} · {String(event.reason)}
            </p>
          ))}
        </section>
      )}
      {detail && detail.cancellations.length > 0 && (
        <section>
          <h2>Cancellation history</h2>
          {detail.cancellations.map((c) => (
            <p key={String(c.id)}>
              Version {String(c.version)} · {String(c.action)} · {String(c.actor_id)} ·{" "}
              {String(c.reason)}
            </p>
          ))}
        </section>
      )}
      {detail &&
        !detail.canCancel &&
        !detail.canReviewCancellation &&
        !detail.canInspect &&
        !detail.canRelease &&
        !detail.canDisposition && (
          <p>
            {detail.hold.status === "disposed"
              ? "This failed stock was scrapped through an independently posted inventory and accounting adjustment."
              : detail.hold.status === "disposition_pending"
                ? "Waiting for Company Finance to review and post the controlled scrap adjustment."
                : detail.hold.status === "cancelled"
                  ? "This hold was cancelled after independent review; no inspection pass was recorded."
                  : detail.hold.status === "released"
                    ? "This hold is released."
                    : "Waiting for an eligible independent quality team member."}
          </p>
        )}
    </section>
  );
}
