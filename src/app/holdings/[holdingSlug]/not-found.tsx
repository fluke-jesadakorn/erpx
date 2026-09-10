import { EmptyState } from "@/components/ui/primitives";
export default function NotFound() {
  return (
    <main className="content">
      <div className="card">
        <EmptyState
          title="This view isn’t available"
          description="The link may be incorrect, or this record isn’t in your current workspace. Use Modules or Work to continue."
        />
      </div>
    </main>
  );
}
