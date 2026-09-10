import { PageHeader, PreviewBanner } from "@/components/ui/primitives";
import styles from "@/components/workplace/pages.module.css";
const lessons = [
  [
    "receiving",
    "Receiving essentials",
    "A careful check at the door keeps the rest of the day moving.",
    "Operations",
  ],
  [
    "handover",
    "A handover that helps",
    "Leave your next teammate with the context they need.",
    "Teamwork",
  ],
  [
    "welcome",
    "Welcome to your workspace",
    "Get familiar with a more connected way to work.",
    "Getting started",
  ],
];
export default function LearnPage() {
  return (
    <main className="content" data-source="sample">
      <PageHeader
        eyebrow="A little learning, every day"
        title="Make room to grow"
        description="Short lessons for the work you do. Practical, approachable, and close at hand."
      />
      <PreviewBanner />
      <section className={styles.learningGrid}>
        {lessons.map(([id, title, description, category]) => (
          <article key={id} className={styles.lesson}>
            <video controls preload="metadata" playsInline aria-label={title}>
              <source src={`/preview/lessons/${id}.mp4`} type="video/mp4" />
              <track
                default
                kind="captions"
                srcLang="en"
                label="English"
                src={`/preview/lessons/${id}.vtt`}
              />
            </video>
            <div>
              <span className="badge">{category}</span>
              <h2>{title}</h2>
              <p>{description}</p>
              <span className="small">
                12 seconds · English captions · Sample lesson
              </span>
            </div>
          </article>
        ))}
      </section>
    </main>
  );
}
