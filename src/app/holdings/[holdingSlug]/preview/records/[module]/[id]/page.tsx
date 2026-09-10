import Link from "next/link";
import { notFound } from "next/navigation";
import { ArrowLeft, ArrowUpRight, Link2 } from "lucide-react";
import {
  findModule,
  moduleHref,
  previewRecordHref,
} from "@/data/module-registry";
import { getModuleSamples } from "@/data/preview/samples";
import {
  PageHeader,
  PreviewBanner,
  StatusBadge,
} from "@/components/ui/primitives";
import styles from "@/components/modules/modules.module.css";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
export default async function PreviewRecordPage({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/preview/records/[module]/[id]">) {
  const { holdingSlug, module: id, id: recordId } = await params;
  const context = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(context));
  const search = await searchParams;
  const definition = findModule(id);
  const samples = getModuleSamples(
    id,
    recordId.startsWith("sample--") ? recordId.split("--")[1] : undefined,
  );
  const record = samples?.records.find((row) => row.id === recordId);
  if (!definition || !samples || !record) notFound();
  const section =
    definition.sections.find((item) => item.id === search.section)?.id ??
    (recordId.startsWith("sample--")
      ? recordId.split("--")[1]
      : definition.primary[0]);
  const view =
    search.view === "form" || search.view === "review"
      ? search.view
      : "details";
  const sectionLabel =
    definition.sections.find((item) => item.id === section)?.label ??
    definition.name;
  const formTitle = recordId.startsWith("sample--")
    ? `${sectionLabel} form`
    : definition.action;
  const filter = new URLSearchParams();
  for (const key of ["q", "status", "sort"])
    if (typeof search[key] === "string") filter.set(key, search[key] as string);
  const tabHref = (tab: string) =>
    `?${new URLSearchParams({ section, view: tab, ...Object.fromEntries(filter) })}`;
  return (
    <main
      className="content"
      data-source="sample"
      data-preview-record={record.id}
    >
      <Link
        className="back-link"
        href={`${moduleHref(base, definition, section)}?${filter}`}
      >
        <ArrowLeft size={14} />
        Back to {definition.name.toLowerCase()}
      </Link>
      <PreviewBanner />
      <PageHeader
        eyebrow={`${definition.name} / ${record.id}`}
        title={record.title}
        description={record.description}
        action={<StatusBadge status={record.status} />}
      />
      <nav className={styles.tabs} aria-label="Sample record views">
        {[
          ["details", "Record details"],
          ["form", "Form preview"],
          ["review", "Review & workflow"],
        ].map(([id, label]) => (
          <Link
            key={id}
            href={tabHref(id)}
            aria-current={view === id ? "page" : undefined}
          >
            {label}
          </Link>
        ))}
      </nav>
      <div className={styles.recordGrid}>
        <section className={styles.primary}>
          <header className={styles.primaryHead}>
            <h2>
              {view === "form"
                ? formTitle
                : view === "review"
                  ? "How this work moves forward"
                  : "Record information"}
            </h2>
            <span>Read-only sample</span>
          </header>
          {view === "details" ? (
            <dl className={styles.detailFields}>
              <div>
                <dt>Reference</dt>
                <dd>{record.id}</dd>
              </div>
              <div>
                <dt>Current status</dt>
                <dd>{record.status}</dd>
              </div>
              {record.fields.map((field) => (
                <div key={field.key}>
                  <dt>{field.label}</dt>
                  <dd>
                    {typeof field.value === "boolean"
                      ? field.value
                        ? "Yes"
                        : "No"
                      : String(field.value) || "Not provided"}
                  </dd>
                </div>
              ))}
            </dl>
          ) : view === "form" ? (
            <div className={styles.formPreview}>
              <p className="small">
                This is a preview of the fields used by this workflow. Editing
                and submission are not available.
              </p>
              {record.fields.map((field) => (
                <label className="field" key={field.key}>
                  {field.label}
                  {field.type === "textarea" ? (
                    <textarea disabled value={String(field.value)} />
                  ) : field.type === "select" ? (
                    <select disabled value={String(field.value)}>
                      {field.options.map((option) => (
                        <option key={option}>{option}</option>
                      ))}
                    </select>
                  ) : field.type === "checkbox" ? (
                    <input
                      disabled
                      type="checkbox"
                      checked={Boolean(field.value)}
                    />
                  ) : (
                    <input
                      disabled
                      type={
                        ["number", "date"].includes(field.type)
                          ? field.type
                          : "text"
                      }
                      value={String(field.value)}
                    />
                  )}
                </label>
              ))}
              <button className="button" disabled>
                {formTitle} · Unavailable in preview
              </button>
            </div>
          ) : (
            <>
              <ol className={styles.steps}>
                {samples.steps.length ? (
                  samples.steps.map((step, index) => (
                    <li key={`${step.action}-${index}`}>
                      <span>{index + 1}</span>
                      <div>
                        <strong>{step.action}</strong>
                        <p>{step.help}</p>
                        <span className="badge">
                          {step.status} → {step.next}
                        </span>
                        {step.handoff && <p>Next handoff: {step.handoff}</p>}
                      </div>
                    </li>
                  ))
                ) : (
                  <li>
                    <span>1</span>
                    <div>
                      <strong>Inspect the source records</strong>
                      <p>
                        This workspace combines information from its owning
                        sources.
                      </p>
                    </div>
                  </li>
                )}
              </ol>
              <div className={styles.reviewFoot}>
                <button className="button" disabled>
                  Confirm action · Unavailable in preview
                </button>
              </div>
            </>
          )}
          <p className={styles.disabledNote}>
            Sample records are separate from your company’s live work. No
            changes can be submitted from this view.
          </p>
        </section>
        <aside className={styles.side}>
          <section>
            <h2>Connected sample records</h2>
            {samples.relations.length ? (
              samples.relations.map((relation) => (
                <Link
                  key={`${relation.moduleId}-${relation.recordId}`}
                  href={previewRecordHref(
                    base,
                    relation.moduleId,
                    relation.recordId,
                  )}
                  className={styles.sideLink}
                >
                  <Link2 size={14} />
                  <span>
                    <strong>{relation.title}</strong>
                    <small>{relation.recordId}</small>
                  </span>
                </Link>
              ))
            ) : (
              <p>No related sample records.</p>
            )}
          </section>
          <section>
            <h2>In this workspace</h2>
            {samples.records
              .filter((row) => row.id !== record.id)
              .map((row) => (
                <Link
                  key={row.id}
                  className={styles.sideLink}
                  href={previewRecordHref(base, definition.id, row.id)}
                >
                  <span>
                    <strong>{row.title}</strong>
                    <small>{row.id}</small>
                  </span>
                  <ArrowUpRight size={13} />
                </Link>
              ))}
          </section>
        </aside>
      </div>
    </main>
  );
}
