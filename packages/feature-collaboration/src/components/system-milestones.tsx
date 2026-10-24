import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import Link from "@erpx/workplace/components/workspace-link";
import type {HomePost} from '@erpx/server-collaboration/public/social-home';
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import styles from "@erpx/feature-collaboration/components/social.module.css";

/** The latest milestone per source keeps repeated system updates in context. */
export function SystemMilestones({ base, posts }: { base: string; posts: HomePost[] }) {
  if (!posts.length) return null;
  const contexts = new Map<string, { post: HomePost; count: number }>();
  for (const post of posts) {
    const source = post.linkedSources.find(source=>source.available);
    const key = source?.available ? `${source.kind}:${source.id}` : post.id;
    const previous = contexts.get(key);
    contexts.set(key, previous ? { ...previous, count: previous.count + 1 } : { post, count: 1 });
  }
  return <details className={styles.milestones}>
    <summary>Recent work milestones <span>{contexts.size}</span></summary>
    <ul>{[...contexts.values()].map(({post,count})=><li key={post.id}>
      <Link href={canonicalUiHref(`${base}/posts/${post.id}`)}>{post.body.length > 180 ? `${post.body.slice(0,180)}…` : post.body}</Link>
      <small><TimeText appearance="standard" value={post.created_at}/>{count>1?` · ${count} recent updates in this context`:""}</small>
      {post.linkedSources.filter(source=>source.available).map(source=>source.available && <Link key={`${source.kind}:${source.id}`} href={`${base}${source.href}`}>Open linked work · {source.label} ↗</Link>)}
    </li>)}</ul>
  </details>;
}
