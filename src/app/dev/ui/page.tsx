import { notFound } from "next/navigation";
import {
  ActionLink,
  Alert,
  Button,
  Card,
  Checkbox,
  Field,
  IconButton,
  LoadingState,
  Radio,
  ResponsiveActions,
  SearchField,
  Select,
  TabLink,
  TableFrame,
  Tabs,
  TextArea,
  TextInput,
  Toolbar,
} from "@/components/ui/foundation";
import { EmptyState, PageHeader, StatusBadge } from "@/components/ui/primitives";
import { Plus, Trash2 } from "lucide-react";
import styles from "./ui-gallery.module.css";

export default function UiGalleryPage() {
  if (process.env.NODE_ENV !== "development") notFound();
  return <main className={styles.page}>
    <PageHeader eyebrow="Development only" title="ERPX interface foundation" description="Canonical controls and states for product migrations. This route is unavailable in production." action={<Button><Plus size={16} />Primary action</Button>} />
    <Tabs label="Foundation sections"><TabLink href="#actions" current>Actions</TabLink><TabLink href="#forms">Forms</TabLink><TabLink href="#feedback">Feedback</TabLink><TabLink href="#data">Data</TabLink></Tabs>

    <section id="actions" className={styles.section}><h2>Actions</h2><Card>
      <Toolbar><Button size="compact">Compact</Button><Button>Primary</Button><Button variant="secondary">Secondary</Button><Button variant="ghost">Ghost</Button><Button variant="danger">Delete</Button><Button pending>Saving</Button><Button disabled>Unavailable</Button><IconButton label="Delete record" tone="danger"><Trash2 size={17} /></IconButton></Toolbar>
    </Card></section>

    <section id="forms" className={styles.section}><h2>Forms</h2><Card><div className={styles.formGrid}>
      <Field label="Legal name" required hint="Use the name shown on official documents."><TextInput required defaultValue="Nisa Chen · นิศา เฉิน" /></Field>
      <Field label="Team"><Select defaultValue="finance"><option value="finance">Finance</option><option value="people">People</option></Select></Field>
      <Field label="Reference" error="This reference already exists."><TextInput invalid defaultValue="INV-2048" /></Field>
      <Field label="Search"><SearchField aria-label="Search records" placeholder="Search records" /></Field>
      <Field label="Notes" className={styles.full}><TextArea defaultValue="Keep enough context for the next person to continue the work." /></Field>
      <div><Checkbox label="Notify the owner" description="Send one update when the record changes." defaultChecked /><Radio name="scope" label="Current workspace" defaultChecked /><Radio name="scope" label="All authorized workspaces" /></div>
    </div></Card></section>

    <section id="feedback" className={styles.section}><h2>Feedback and states</h2><div className={styles.stack}>
      <Alert tone="success" title="Changes saved">The latest version is available to your team.</Alert>
      <Alert tone="warning" title="Review required">Two fields need confirmation before submission.</Alert>
      <Alert tone="danger" title="Could not save" live>Your edits are still here. Check the connection and try again.</Alert>
      <Alert tone="info" title="Limited access">You can view this record, but only Finance can approve it.</Alert>
      <Card><div className={styles.badges}><StatusBadge status="active" /><StatusBadge status="pending review" /><StatusBadge status="failed" /></div></Card>
      <Card><LoadingState label="Loading workspace" /></Card>
      <Card><EmptyState title="No results" description="Try changing the search or clearing the current filters." action={<ActionLink href="#forms">Clear filters</ActionLink>} /></Card>
    </div></section>

    <section id="data" className={styles.section}><h2>Responsive data</h2><TableFrame label="Example invoices"><table><thead><tr><th>Invoice</th><th>Customer</th><th>Status</th><th>Amount</th></tr></thead><tbody><tr><td>INV-2048</td><td>Bangkok Operations</td><td><StatusBadge status="pending" /></td><td>฿24,900.00</td></tr><tr><td>INV-2047</td><td>Long organization name for responsive testing</td><td><StatusBadge status="approved" /></td><td>฿8,400.00</td></tr></tbody></table></TableFrame></section>
    <ResponsiveActions><Button variant="secondary">Save draft</Button><Button>Continue</Button></ResponsiveActions>
  </main>;
}
