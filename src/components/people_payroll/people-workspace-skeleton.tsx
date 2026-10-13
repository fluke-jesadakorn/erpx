import layout from "@/components/organization/organization-structure.module.css";
import styles from "@/components/people_payroll/people-workspace-skeleton.module.css";

function Bar({ width = "100%", height = 14 }: { width?: string; height?: number }) {
  return <span className={styles.bar} style={{ width, height }} />;
}

export function PeopleWorkspaceSkeleton({ canManage, canOpenHrSettings }: {
  canManage: boolean;
  canOpenHrSettings: boolean;
}) {
  return (
    <section className={layout.workspace} aria-busy="true" data-testid="people-workspace-skeleton">
      <span className="sr-only" role="status">Loading people and organization…</span>
      <div aria-hidden="true">
        <div className={layout.toolbar}>
          <div className={`${layout.search} ${styles.search}`}><Bar width="70%" /></div>
          <div className={layout.topActions}>
            {canManage && <Bar width="68px" height={44} />}
            {canOpenHrSettings && <Bar width="132px" height={44} />}
          </div>
        </div>
        <div className={layout.layout}>
          <div className={layout.unitPicker}><Bar width="160px" height={24} /></div>
          <div className={`${layout.rail} ${styles.rail}`}>
            <div className={layout.railTitle}><Bar width="50px" /><Bar width="100px" height={44} /></div>
            {Array.from({ length: 7 }, (_, index) => (
              <div className={styles.unit} key={index} style={{ marginLeft: index > 1 ? 20 : 0 }}>
                <Bar width={index % 2 ? "75%" : "90%"} height={16} />
              </div>
            ))}
          </div>
          <div className={layout.main}>
            <div className={`${layout.locationBar} ${styles.location}`}><Bar width="50px" height={10} /><Bar width="140px" /></div>
            <div className={layout.mainHeading}>
              <div className={styles.heading}><Bar width="130px" height={11} /><Bar width="min(260px, 100%)" height={28} /><Bar width="170px" height={13} /></div>
              <Bar width="114px" height={44} />
            </div>
            <div className={layout.occupancy}>{[72, 88, 88].map((width, index) => <Bar key={index} width={`${width}px`} height={44} />)}</div>
            <div className={layout.group}>
              <div className={`${layout.groupHeading} ${styles.groupHeading}`}><Bar width="180px" height={16} /><Bar width="240px" height={12} /></div>
              {Array.from({ length: 5 }, (_, index) => (
                <div className={styles.row} key={index}>
                  <div className={styles.position}><Bar width="min(200px, 85%)" height={16} /><Bar width="min(250px, 95%)" height={12} /></div>
                  <div className={styles.person}><Bar width="32px" height={32} /><Bar width="min(140px, 65%)" /></div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
