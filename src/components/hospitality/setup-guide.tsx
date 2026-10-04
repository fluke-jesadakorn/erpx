"use client";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";

import { useEffect, useRef, useState, type FormEvent } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import type { HospitalitySetup } from "@/server/hospitality-setup";
import { records as pmsRecords, type Kind } from "@/lib/pms-model";
import { workFetch } from "@/lib/work-fetch";
import {
  expandRoomNumbers,
  roomSaveKey,
  mergeSetupSave,
  type SetupSave,
} from "@/lib/hospitality-grid";
import styles from "./setup-guide.module.css";
import { FrontLanguageContext, FrontLocalizedTree } from "@/components/front/front-language";

export function HospitalitySetupGuide({
  initial,
  base,
}: {
  initial: HospitalitySetup;
  base: string;
}) {
  const router = useRouter();
  const [data, setData] = useState(initial),
    [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [notice, setNotice] = useState(""),
    [language, setLanguage] = useState<"en" | "th">("en");
  const [jobs, setJobs] = useState<SetupSave[]>([]),
    jobRef = useRef<SetupSave[]>([]);
  const [menu, setMenu] = useState([
    {
      sku: "",
      name: "",
      price: "",
      kind: "stock",
      station: "Kitchen",
      ingredient: "",
      quantity: "",
      key: crypto.randomUUID(),
    },
  ]);
  const [tableRows, setTableRows] = useState([{ name: "", seats: 4, key: crypto.randomUUID() }]);
  const [chartId, setChartId] = useState(
    initial.hotel?.property.config.chartId ||
      String(initial.charts.find((c) => c.base_currency === "THB")?.id ?? ""),
  );
  const [roomRows, setRoomRows] = useState([
    { numbers: "", typeId: "", floor: "", key: crypto.randomUUID() },
  ]);
  const storage = `hospitality-setup:${base}:${data.companyId}:${data.branchId}:${data.propertyId ?? "new"}`;
  const api = base.replace("/holdings/", "/api/v1/holdings/");
  const tr = (en: string, th: string) => (language === "th" ? th : en);
  useEffect(() => {
    queueMicrotask(() => {
      try {
        const stored = JSON.parse(localStorage.getItem(storage) ?? "[]") as SetupSave[];
        jobRef.current = stored;
        setJobs(stored);
      } catch {}
    });
  }, [storage]);
  function persist(next: SetupSave[]) {
    jobRef.current = next;
    setJobs([...next]);
    localStorage.setItem(storage, JSON.stringify(next));
  }
  async function refresh(property = String(data.propertyId ?? "")) {
    const response = await workFetch(
      `${api}/hospitality/setup?branch=${encodeURIComponent(data.branchId)}&property=${encodeURIComponent(property)}`,
    );
    const body = await response.json();
    if (!response.ok) throw new Error(body.error?.message ?? "Refresh failed");
    setData(body.data);
    return body.data as HospitalitySetup;
  }
  async function request(job: SetupSave) {
    const response = await workFetch(job.url, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Idempotency-Key": job.requestKey ?? job.key },
      body: JSON.stringify(job.body),
    });
    const result = await response.json();
    if (!response.ok)
      throw Object.assign(new Error(result.error?.message ?? "Save failed"), {
        code: result.error?.code,
        rejected:
          response.status >= 400 &&
          response.status < 500 &&
          response.status !== 408 &&
          !["WORK_CONTEXT_CHANGED", "IDEMPOTENCY_CONFLICT"].includes(result.error?.code),
      });
    return result.data as { id: string; version?: number };
  }
  async function runJobs(next: SetupSave[]) {
    setBusy(true);
    setError("");
    persist(next);
    try {
      for (const job of next.filter((j) => j.state !== "saved")) {
        // A saved request body is retained after an interrupted response for exact replay.
        try {
          if (job.dependsOn) {
            const dependency = next.find((j) => j.key === job.dependsOn);
            if (!dependency?.resultId) throw new Error("Save the product row first.");
            job.body.productId = dependency.resultId;
          }
          if (job.resolveHotelVersion) {
            const fresh = await refresh();
            job.body.expectedVersion = fresh.hotel?.property.version;
            delete job.resolveHotelVersion;
          }
          persist([...next]);
          let result;
          try {
            result = await request(job);
          } catch (e) {
            if ((e as { code?: string }).code !== "PMS_VERSION_CONFLICT") throw e;
            const fresh = await refresh();
            job.requestKey = crypto.randomUUID();
            job.body.expectedVersion = fresh.hotel?.property.version;
            persist([...next]);
            result = await request(job);
          }
          job.resultId = result.id;
          job.state = "saved";
          delete job.error;
          persist([...next]);
          await refresh();
        } catch (e) {
          job.state = "failed";
          job.error = e instanceof Error ? e.message : "Save failed";
          job.rejected = Boolean((e as { rejected?: boolean }).rejected);
          persist([...next]);
        }
      }
      setNotice(
        tr(
          "Saved rows remain saved. Review failed rows before retrying.",
          "แถวที่บันทึกแล้วจะไม่บันทึกซ้ำ กรุณาตรวจแถวที่ไม่สำเร็จก่อนลองใหม่",
        ),
      );
    } finally {
      setBusy(false);
    }
  }
  async function submit(
    operation: string,
    input: Record<string, unknown>,
    key = crypto.randomUUID(),
  ) {
    const fresh = await refresh();
    if (operation === "property.save" && fresh.hotel)
      input = {
        name: fresh.hotel.property.name,
        country: fresh.hotel.property.config.country,
        timezone: fresh.hotel.property.config.timezone,
        ...input,
      };
    const job: SetupSave = {
      key,
      url: `${api}/pms`,
      body: {
        operation,
        input,
        ...(operation === "property.create"
          ? {}
          : { propertyId: fresh.propertyId, expectedVersion: fresh.hotel?.property.version }),
      },
      label: operation,
      state: "pending",
    };
    setBusy(true);
    setError("");
    try {
      const result = await request(job);
      await refresh(operation === "property.create" ? result.id : undefined);
      setNotice(tr("Saved", "บันทึกแล้ว"));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setBusy(false);
    }
  }
  const rows = <K extends Kind>(kind: K) =>
    data.hotel
      ? pmsRecords({ property: data.hotel.property, records: data.hotel.records }, kind)
      : [];
  const formData = (e: FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    return new FormData(e.currentTarget);
  };
  const saveRooms = async () => {
    setError("");
    try {
      const pending = (
        await Promise.all(
          roomRows.flatMap((r) =>
            expandRoomNumbers(r.numbers).map(async (number) => ({
              row: r,
              number,
              key: await roomSaveKey(r.key, number),
            })),
          ),
        )
      ).filter((r) => !jobRef.current.some((j) => j.key === r.key && j.state === "saved"));
      const numbers = pending.map((r) => r.number);
      if (new Set(numbers).size !== numbers.length)
        throw new Error("Duplicate room numbers in the grid.");
      if (pending.some((r) => !r.row.typeId)) throw new Error("Choose a room type for every row.");
      if (numbers.some((n) => rows("room").some((r) => r.data.number === n)))
        throw new Error("A room number already exists. Edit it in Advanced settings.");
      const next = [...jobRef.current];
      for (const { row, number, key } of pending)
        mergeSetupSave(next, {
          key,
          url: `${api}/pms`,
          body: {
            operation: "room.save",
            propertyId: data.propertyId,
            input: { number, typeId: row.typeId, floor: row.floor, active: true },
          },
          label: `Room ${number}`,
          state: "pending",
          resolveHotelVersion: true,
        });
      await runJobs(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
    }
  };
  async function saveMenu() {
    setError("");
    try {
      if (new Set(menu.map((r) => r.sku.trim())).size !== menu.length)
        throw new Error("Duplicate menu SKUs.");
      for (const row of menu) {
        if (
          !row.sku.trim() ||
          !row.name.trim() ||
          !row.price ||
          !Number.isFinite(Number(row.price)) ||
          Number(row.price) <= 0
        )
          throw new Error("Enter each menu name, SKU and a positive price.");
        if (
          row.kind === "recipe" &&
          (!row.ingredient || !row.quantity || Number(row.quantity) <= 0)
        )
          throw new Error("Enter the recipe ingredient and quantity.");
      }
      const next = [...jobRef.current];
      for (const row of menu) {
        const productKey = `${row.key}:product`,
          catalogKey = `${row.key}:catalog`;
        mergeSetupSave(next, {
          key: productKey,
          url: `${api}/commerce/products`,
          body: { sku: row.sku, name: row.name, price: row.price, unit: "each", taxBps: 0 },
          label: row.name,
          state: "pending",
        });
        mergeSetupSave(next, {
          key: catalogKey,
          url: `${api}/pos/catalog/save`,
          body: {
            kind: row.kind,
            category: "Restaurant",
            station: row.station,
            barcode: "",
            recipe:
              row.kind === "recipe"
                ? [
                    {
                      productId: row.ingredient,
                      quantityMilli: Math.round(Number(row.quantity) * 1000),
                    },
                  ]
                : [],
            modifiers: [],
            taxInclusive: false,
            expectedVersion: 0,
          },
          label: `Menu ${row.name}`,
          state: "pending",
          dependsOn: productKey,
        });
      }
      await runJobs(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
    }
  }
  return (
    <FrontLanguageContext.Provider value={language}>
      <FrontLocalizedTree>
        <div className={styles.guide}>
          <header className={styles.header}>
            <div>
              <h1>{tr("Hotel + Restaurant Setup", "ตั้งค่าโรงแรมและร้านอาหาร")}</h1>
              <p>
                {data.companyName} ·{" "}
                {data.ready
                  ? tr("Ready for rehearsal", "พร้อมสำหรับการฝึกซ้อม")
                  : tr("Preparing for rehearsal", "เตรียมการฝึกซ้อม")}
              </p>
              <p>
                {tr(
                  "Progress comes from saved records. Reopen this guide to continue.",
                  "ความคืบหน้ามาจากข้อมูลที่บันทึก เปิดคู่มือนี้อีกครั้งเพื่อทำต่อ",
                )}
              </p>
            </div>
            <Button
              type="submit"
              variant="plain"
              onClick={() => setLanguage(language === "en" ? "th" : "en")}
            >
              {language === "en" ? "ไทย" : "English"}
            </Button>
          </header>
          {error && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}
          {notice && (
            <p className={styles.notice} role="status">
              {notice}
            </p>
          )}
          <section className={styles.card}>
            <h2>1. {tr("Choose location", "เลือกสาขา")}</h2>
            <p>
              {tr(
                "Use the account switcher to choose an existing company.",
                "ใช้เมนูบัญชีเพื่อเลือกบริษัทที่มีอยู่",
              )}
            </p>
            <Field>
              {tr("Hotel branch", "สาขาโรงแรม")}{" "}
              <Select
                value={data.branchId}
                disabled={busy}
                onChange={(e) => {
                  router.push(
                    `${base}/hospitality/setup?branch=${encodeURIComponent(e.target.value)}`,
                  );
                }}
              >
                {data.branches.map((b) => (
                  <option key={String(b.id)} value={String(b.id)}>
                    {b.name}
                  </option>
                ))}
              </Select>
            </Field>
            {data.properties.length > 0 && (
              <Field>
                {" "}
                Hotel{" "}
                <Select
                  value={String(data.propertyId ?? "")}
                  onChange={(e) => void refresh(e.target.value).catch((e) => setError(e.message))}
                >
                  {data.properties.map((p) => (
                    <option key={String(p.id)} value={String(p.id)}>
                      {p.name}
                    </option>
                  ))}
                </Select>
              </Field>
            )}
          </section>
          <div className={styles.steps}>
            {data.steps.slice(1).map((step, i) => (
              <section className={styles.card} key={step.id}>
                <h2>
                  {i + 2}.{" "}
                  {tr(
                    step.title,
                    [
                      "เตรียมบัญชีและสต็อก",
                      "ตั้งค่าโรงแรม",
                      "ตั้งค่าร้านอาหาร",
                      "เตรียมทีมงาน",
                      "ตรวจสอบและเปิดใช้งาน",
                    ][i],
                  )}
                </h2>
                <small>{step.owner}</small>
                {step.blockers.length ? (
                  <ul>
                    {step.blockers.map((b) => (
                      <li key={b}>{b}</li>
                    ))}
                  </ul>
                ) : (
                  <p className={styles.ready}>{tr("Complete", "ครบแล้ว")}</p>
                )}
                <Link
                  href={`${base}${step.href}${step.id === "hotel" || step.id === "open" ? `?property=${data.propertyId ?? ""}` : ""}`}
                >
                  {tr("Open settings / next action", "เปิดการตั้งค่า / ขั้นตอนถัดไป")}
                </Link>
              </section>
            ))}
          </div>
          {data.permissions.financePrepare && (
            <section className={styles.card}>
              <h2>
                {tr(
                  "Finance review: suggested hotel accounts",
                  "ฝ่ายบัญชี: ตรวจสอบบัญชีโรงแรมที่แนะนำ",
                )}
              </h2>
              <p>
                {tr(
                  "Review every mapping and tax setting. Empty mappings create dedicated THB accounts. Another person must approve in PMS Settings.",
                  "ตรวจสอบการจับคู่บัญชีและภาษีทุกช่อง ช่องว่างจะสร้างบัญชี THB โดยเฉพาะ และต้องให้บุคคลอื่นอนุมัติในตั้งค่า PMS",
                )}
              </p>
              <form
                onSubmit={(e) => {
                  const f = formData(e);
                  void submit("accounting.bootstrap", {
                    chartId: f.get("chartId"),
                    accounts: Object.fromEntries(
                      data.suggestions
                        .map((s) => [s.key, String(f.get(s.key) ?? "")])
                        .filter(([, v]) => v),
                    ),
                    taxBps: Math.round(Number(f.get("tax")) * 100),
                    serviceBps: Math.round(Number(f.get("service")) * 100),
                    taxInclusive: f.has("inclusive"),
                    advanceTax: f.has("advance"),
                    financeReviewed: f.has("review"),
                  });
                }}
              >
                <div className={styles.form}>
                  <Field>
                    THB chart
                    <Select
                      name="chartId"
                      value={chartId}
                      onChange={(e) => setChartId(e.target.value)}
                      required
                    >
                      {data.charts
                        .filter((c) => c.base_currency === "THB")
                        .map((c) => (
                          <option key={String(c.id)} value={String(c.id)}>
                            {c.name}
                          </option>
                        ))}
                    </Select>
                  </Field>
                  <Field>
                    Tax (%)
                    <TextInput name="tax" type="number" min="0" max="100" step="0.01" required />
                  </Field>
                  <Field>
                    Service charge (%)
                    <TextInput
                      name="service"
                      type="number"
                      min="0"
                      max="100"
                      step="0.01"
                      required
                    />
                  </Field>
                </div>
                <TableFrame label="Records" className={styles.scroll}>
                  <Table>
                    <thead>
                      <tr>
                        <th>Mapping</th>
                        <th>Finance account</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.suggestions.map((s) => (
                        <tr key={s.key}>
                          <td>
                            {s.key} · {s.category}
                          </td>
                          <td>
                            <Select
                              name={s.key}
                              defaultValue={data.hotel?.property.config.accounts[s.key] ?? ""}
                            >
                              <option value="">Create / reuse {s.code}</option>
                              {data.accounts
                                .filter(
                                  (a) =>
                                    a.chart_id === chartId &&
                                    a.currency === "THB" &&
                                    a.category === s.category,
                                )
                                .map((a) => (
                                  <option key={String(a.id)} value={String(a.id)}>
                                    {a.code} · {a.name}
                                  </option>
                                ))}
                            </Select>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </TableFrame>
                <div className={styles.form}>
                  <Field orientation="inline" className={styles.check}>
                    <TextInput type="checkbox" name="inclusive" />
                    Prices include tax and service
                  </Field>
                  <Field orientation="inline" className={styles.check}>
                    <TextInput type="checkbox" name="advance" />
                    Tax advance receipts
                  </Field>
                  <Field orientation="inline" className={styles.check}>
                    <TextInput type="checkbox" name="review" required />I reviewed account and tax
                    settings
                  </Field>
                </div>
                <Button clientManaged type="submit" variant="plain" disabled={busy || !data.propertyId}>
                  Prepare hotel accounts
                </Button>
              </form>
            </section>
          )}
          {data.permissions.hotel && (
            <section className={styles.card}>
              <h2>{tr("Hotel entry grid", "ตารางตั้งค่าโรงแรม")}</h2>
              {!data.hotel ? (
                <form
                  className={styles.form}
                  onSubmit={(e) => {
                    const f = formData(e);
                    void submit("property.create", {
                      name: f.get("name"),
                      branchId: data.branchId,
                      businessDate: f.get("date"),
                    });
                  }}
                >
                  <Field>
                    Hotel name
                    <TextInput name="name" required />
                  </Field>
                  <Field>
                    Business date
                    <TextInput name="date" type="date" defaultValue={data.date} required />
                  </Field>
                  <Button clientManaged type="submit" variant="plain" disabled={busy || !data.branchId}>
                    Create hotel
                  </Button>
                </form>
              ) : (
                <>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      const f = formData(e);
                      void submit("property.save", {
                        checkIn: String(f.get("checkIn")),
                        checkOut: String(f.get("checkOut")),
                        cancellationPolicy: String(f.get("policy")),
                      });
                    }}
                  >
                    <Field>
                      Check in
                      <TextInput
                        name="checkIn"
                        type="time"
                        defaultValue={data.hotel.property.config.checkIn}
                        required
                      />
                    </Field>
                    <Field>
                      Check out
                      <TextInput
                        name="checkOut"
                        type="time"
                        defaultValue={data.hotel.property.config.checkOut}
                        required
                      />
                    </Field>
                    <Field>
                      Cancellation policy
                      <TextInput
                        name="policy"
                        defaultValue={data.hotel.property.config.cancellationPolicy}
                        required
                      />
                    </Field>
                    <Button clientManaged type="submit" variant="plain" disabled={busy}>
                      Save hotel policy
                    </Button>
                  </form>
                  <form
                    className={styles.form}
                    onSubmit={(e) => {
                      const f = formData(e);
                      void submit("room_type.save", {
                        name: f.get("name"),
                        capacity: Number(f.get("capacity")),
                      });
                    }}
                  >
                    <Field>
                      Room type
                      <TextInput name="name" placeholder="Standard / Deluxe" required />
                    </Field>
                    <Field>
                      Guests
                      <TextInput
                        name="capacity"
                        type="number"
                        min="1"
                        max="50"
                        defaultValue="2"
                        required
                      />
                    </Field>
                    <Button clientManaged type="submit" variant="plain" disabled={busy}>
                      Add room type
                    </Button>
                  </form>
                  <TableFrame label="Records" className={styles.scroll}>
                    <Table>
                      <thead>
                        <tr>
                          <th>Room numbers / range</th>
                          <th>Room type</th>
                          <th>Floor</th>
                        </tr>
                      </thead>
                      <tbody>
                        {roomRows.map((r, i) => (
                          <tr key={r.key}>
                            <td>
                              <TextInput
                                aria-label={`Room numbers ${i + 1}`}
                                placeholder="101-104, 201"
                                value={r.numbers}
                                onChange={(e) =>
                                  setRoomRows(
                                    roomRows.map((x, n) =>
                                      n === i ? { ...x, numbers: e.target.value } : x,
                                    ),
                                  )
                                }
                              />
                            </td>
                            <td>
                              <Select
                                aria-label={`Room type ${i + 1}`}
                                value={r.typeId}
                                onChange={(e) =>
                                  setRoomRows(
                                    roomRows.map((x, n) =>
                                      n === i ? { ...x, typeId: e.target.value } : x,
                                    ),
                                  )
                                }
                              >
                                <option value="">Choose type</option>
                                {rows("room_type").map((t) => (
                                  <option key={t.id} value={t.id}>
                                    {t.data.name}
                                  </option>
                                ))}
                              </Select>
                            </td>
                            <td>
                              <TextInput
                                aria-label={`Floor ${i + 1}`}
                                value={r.floor}
                                onChange={(e) =>
                                  setRoomRows(
                                    roomRows.map((x, n) =>
                                      n === i ? { ...x, floor: e.target.value } : x,
                                    ),
                                  )
                                }
                              />
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </TableFrame>
                  <div className={styles.form}>
                    <Button
                      type="submit"
                      variant="plain"
                      disabled={busy}
                      onClick={() =>
                        setRoomRows([
                          ...roomRows,
                          { numbers: "", typeId: "", floor: "", key: crypto.randomUUID() },
                        ])
                      }
                    >
                      Add room row
                    </Button>
                    <Button
                      type="submit"
                      variant="plain"
                      disabled={busy}
                      onClick={() => void saveRooms()}
                    >
                      Save rooms
                    </Button>
                  </div>
                  <p>
                    Saved rooms:{" "}
                    {rows("room")
                      .map((r) => r.data.number)
                      .join(", ") || "None"}
                  </p>
                  <p>
                    Saved rates:{" "}
                    {rows("rate")
                      .map(
                        (r) =>
                          `${r.data.name} · THB ${(r.data.price / 100).toFixed(2)} · ${r.data.from} – ${r.data.to}`,
                      )
                      .join(", ") || "None"}
                  </p>
                  {rows("room_type").map((t) => (
                    <form
                      key={t.id}
                      className={styles.form}
                      onSubmit={(e) => {
                        const f = formData(e);
                        void submit("rate.save", {
                          typeId: t.id,
                          name: `${t.data.name} nightly`,
                          from: f.get("from"),
                          to: f.get("to"),
                          price: Math.round(Number(f.get("price")) * 100),
                        });
                      }}
                    >
                      <strong>{t.data.name}</strong>
                      <Field>
                        Nightly price (THB)
                        <TextInput name="price" type="number" min="0.01" step="0.01" required />
                      </Field>
                      <Field>
                        From
                        <TextInput name="from" type="date" defaultValue={data.date} required />
                      </Field>
                      <Field>
                        To
                        <TextInput name="to" type="date" required />
                      </Field>
                      <Button clientManaged type="submit" variant="plain" disabled={busy}>
                        Save nightly rate
                      </Button>
                    </form>
                  ))}
                </>
              )}
              <p>
                <Link href={`${base}/pms/settings?property=${data.propertyId ?? ""}`}>
                  Advanced PMS settings · edit saved rooms and rates
                </Link>
              </p>
            </section>
          )}
          {data.permissions.restaurant && (
            <section className={styles.card}>
              <h2>{tr("Restaurant entry grid", "ตารางตั้งค่าร้านอาหาร")}</h2>
              {!data.registers.length && (
                <form
                  className={styles.form}
                  onSubmit={(e) => {
                    const f = formData(e);
                    void runJobs([
                      ...jobRef.current,
                      {
                        key: crypto.randomUUID(),
                        url: `${api}/pos/${data.posConfigured ? "register/save" : "settings/bootstrap"}`,
                        body: {
                          mode: "restaurant",
                          ...(data.posConfigured
                            ? { name: f.get("name") }
                            : { registerName: f.get("name") }),
                          warehouseId: f.get("warehouse"),
                        },
                        label: "Restaurant setup",
                        state: "pending",
                      },
                    ]);
                  }}
                >
                  <Field>
                    Register name
                    <TextInput name="name" defaultValue="Main restaurant register" required />
                  </Field>
                  <Field>
                    Kitchen warehouse
                    <Select name="warehouse" required>
                      {data.warehouses.map((w) => (
                        <option key={String(w.id)} value={String(w.id)}>
                          {w.name}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Button clientManaged type="submit" variant="plain" disabled={busy}>
                    Set up restaurant POS
                  </Button>
                </form>
              )}
              <TableFrame label="Records" className={styles.scroll}>
                <Table>
                  <thead>
                    <tr>
                      <th>Table</th>
                      <th>Seats</th>
                    </tr>
                  </thead>
                  <tbody>
                    {tableRows.map((r, i) => (
                      <tr key={r.key}>
                        <td>
                          <TextInput
                            aria-label={`Table name ${i + 1}`}
                            value={r.name}
                            onChange={(e) =>
                              setTableRows(
                                tableRows.map((x, n) =>
                                  n === i ? { ...x, name: e.target.value } : x,
                                ),
                              )
                            }
                          />
                        </td>
                        <td>
                          <TextInput
                            aria-label={`Seats ${i + 1}`}
                            type="number"
                            min="1"
                            max="100"
                            value={r.seats}
                            onChange={(e) =>
                              setTableRows(
                                tableRows.map((x, n) =>
                                  n === i ? { ...x, seats: Number(e.target.value) } : x,
                                ),
                              )
                            }
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
              <div className={styles.form}>
                <Button
                  type="submit"
                  variant="plain"
                  disabled={busy}
                  onClick={() =>
                    setTableRows([...tableRows, { name: "", seats: 4, key: crypto.randomUUID() }])
                  }
                >
                  Add table row
                </Button>
                <Button
                  type="submit"
                  variant="plain"
                  disabled={busy || !data.registers.length}
                  onClick={() => {
                    if (
                      new Set(tableRows.map((r) => r.name.trim())).size !== tableRows.length ||
                      tableRows.some(
                        (r) =>
                          data.tables.some((t) => t.name === r.name.trim()) &&
                          !jobRef.current.some((j) => j.key === r.key && j.state === "saved"),
                      )
                    ) {
                      setError("Duplicate table names.");
                      return;
                    }
                    const next = [...jobRef.current];
                    for (const r of tableRows)
                      mergeSetupSave(next, {
                        key: r.key,
                        url: `${api}/pos/table/create`,
                        body: { registerId: data.registers[0].id, name: r.name, seats: r.seats },
                        label: `Table ${r.name}`,
                        state: "pending",
                      });
                    void runJobs(next);
                  }}
                >
                  Save tables
                </Button>
              </div>
              <p>Saved tables: {data.tables.map((r) => r.name).join(", ") || "None"}</p>
              {!data.permissions.productCreate && (
                <p>
                  {tr(
                    "Inventory preparation access is needed to create menu products here. Ask Inventory to create products in Commerce, then configure the menu in Advanced POS settings.",
                    "การเพิ่มสินค้าเมนูต้องมีสิทธิ์เตรียมสินค้าคงคลัง ให้ฝ่ายคลังสร้างสินค้าใน Commerce แล้วตั้งค่าเมนูในตั้งค่า POS ขั้นสูง",
                  )}
                </p>
              )}
              <p>
                Menu prices exclude tax here. Finance must review tax policy in Advanced settings.
                Add ingredients and opening stock in <Link href={`${base}/commerce`}>Commerce</Link>
                .
              </p>
              <TableFrame label="Records" className={styles.scroll}>
                <Table>
                  <thead>
                    <tr>
                      <th>SKU</th>
                      <th>Menu name</th>
                      <th>THB price</th>
                      <th>Kind</th>
                      <th>Station</th>
                      <th>Ingredient</th>
                      <th>Quantity</th>
                    </tr>
                  </thead>
                  <tbody>
                    {menu.map((r, i) => (
                      <tr key={r.key}>
                        {(["sku", "name", "price"] as const).map((k) => (
                          <td key={k}>
                            <TextInput
                              aria-label={`${k} ${i + 1}`}
                              value={r[k]}
                              onChange={(e) =>
                                setMenu(
                                  menu.map((x, n) => (n === i ? { ...x, [k]: e.target.value } : x)),
                                )
                              }
                            />
                          </td>
                        ))}
                        <td>
                          <Select
                            aria-label={`Menu kind ${i + 1}`}
                            value={r.kind}
                            onChange={(e) =>
                              setMenu(
                                menu.map((x, n) => (n === i ? { ...x, kind: e.target.value } : x)),
                              )
                            }
                          >
                            <option value="stock">Stock item</option>
                            <option value="recipe">Recipe</option>
                          </Select>
                        </td>
                        <td>
                          <TextInput
                            aria-label={`Station ${i + 1}`}
                            value={r.station}
                            onChange={(e) =>
                              setMenu(
                                menu.map((x, n) =>
                                  n === i ? { ...x, station: e.target.value } : x,
                                ),
                              )
                            }
                          />
                        </td>
                        <td>
                          <Select
                            aria-label={`Ingredient ${i + 1}`}
                            value={r.ingredient}
                            onChange={(e) =>
                              setMenu(
                                menu.map((x, n) =>
                                  n === i ? { ...x, ingredient: e.target.value } : x,
                                ),
                              )
                            }
                          >
                            <option value="">Choose</option>
                            {data.products.map((p) => (
                              <option key={String(p.id)} value={String(p.id)}>
                                {p.name}
                              </option>
                            ))}
                          </Select>
                        </td>
                        <td>
                          <TextInput
                            aria-label={`Ingredient quantity ${i + 1}`}
                            value={r.quantity}
                            onChange={(e) =>
                              setMenu(
                                menu.map((x, n) =>
                                  n === i ? { ...x, quantity: e.target.value } : x,
                                ),
                              )
                            }
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
              <div className={styles.form}>
                <Button
                  type="submit"
                  variant="plain"
                  disabled={busy}
                  onClick={() =>
                    setMenu([
                      ...menu,
                      {
                        sku: "",
                        name: "",
                        price: "",
                        kind: "stock",
                        station: "Kitchen",
                        ingredient: "",
                        quantity: "",
                        key: crypto.randomUUID(),
                      },
                    ])
                  }
                >
                  Add menu row
                </Button>
                <Button
                  type="submit"
                  variant="plain"
                  disabled={busy || !data.permissions.productCreate}
                  onClick={() => void saveMenu()}
                >
                  Save menu
                </Button>
              </div>
              <p>
                Saved menu:{" "}
                {data.products
                  .filter((p) => p.kind)
                  .map((p) => `${p.name} · THB ${(Number(p.price_minor) / 100).toFixed(2)}`)
                  .join(", ") || "None"}
              </p>
              <Link href={`${base}/pos?tab=settings`}>
                Advanced POS settings · recipes, accounts, tax and registers
              </Link>
            </section>
          )}
          {jobs.length > 0 && (
            <section className={styles.card}>
              <h2>{tr("Row save results", "ผลการบันทึกแต่ละแถว")}</h2>
              <h3>Saved rows</h3>
              <ul>
                {jobs
                  .filter((j) => j.state === "saved")
                  .map((j) => (
                    <li key={j.key}>{j.label}</li>
                  ))}
              </ul>
              <h3>Failed / interrupted rows</h3>
              <p>
                Edit rejected rows in the entry grid and save again. Interrupted requests retry the
                original data.
              </p>
              <ul>
                {jobs
                  .filter((j) => j.state !== "saved")
                  .map((j) => (
                    <li key={j.key}>
                      {j.label}: {j.error ?? "Response interrupted"}
                    </li>
                  ))}
              </ul>
              <Button
                type="submit"
                variant="plain"
                disabled={busy || jobs.every((j) => j.state === "saved")}
                onClick={() => void runJobs([...jobs])}
              >
                Retry unfinished saves
              </Button>
            </section>
          )}
          <section className={styles.card}>
            <h2>7. {tr("Practise together", "ฝึกซ้อมร่วมกัน")}</h2>
            <p className={data.ready ? styles.ready : undefined}>
              {data.ready
                ? tr("Ready for rehearsal", "พร้อมฝึกซ้อม")
                : tr(
                    "Resolve setup blockers before rehearsal.",
                    "แก้ไขรายการที่ยังไม่พร้อมก่อนฝึกซ้อม",
                  )}
            </p>
            <ol>
              {[
                ["Reserve and check in", "/pms/reservations"],
                ["Open hotel cash session with counted float", "/pms/cashier"],
                ["Open restaurant cash session with counted float", "/pos?tab=shifts"],
                ["Prepare and deliver a restaurant order", "/pos?tab=kitchen"],
                ["Charge the unpaid bill to the checked-in room", "/pos"],
                ["Settle the folio and check out", "/pms/folios"],
                ["Close cash sessions and review Night Audit", "/pms/night-audit"],
              ].map(([name, href]) => (
                <li key={href}>
                  <Link
                    href={`${base}${href}${href.startsWith("/pms/") ? `?property=${data.propertyId ?? ""}` : ""}`}
                  >
                    {name}
                  </Link>
                </li>
              ))}
            </ol>
            <h3>Room charge requirements</h3>
            <ul>
              {data.roomChargeRequirements.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
            <h3>Activation restrictions</h3>
            <ul>
              {data.activationRestrictions.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          </section>
        </div>
      </FrontLocalizedTree>
    </FrontLanguageContext.Provider>
  );
}
