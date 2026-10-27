//! macOS memory observations are evidence only: this module never signals or
//! reaps a PID. The supervisor's owned Tokio Child decides actual exit status.
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, RefreshKind, System};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MemoryObservation {
    Continue,
    Unavailable,
    MemoryLimit,
}

pub(super) fn classify(status: ProcessStatus, memory: u64, ceiling: u64) -> MemoryObservation {
    if memory > ceiling {
        // Preserve every observed excess, including a final zombie sample.
        MemoryObservation::MemoryLimit
    } else if status == ProcessStatus::Zombie {
        // A zombie has exited and cannot allocate. Its zero RSS must not race
        // the owned child's still-pending pipes/wait into "unavailable".
        MemoryObservation::Continue
    } else if memory == 0 {
        // Missing measurements on a live/unknown process remain fail closed.
        MemoryObservation::Unavailable
    } else {
        MemoryObservation::Continue
    }
}

pub(super) fn sample(pid: u32, ceiling: u64) -> MemoryObservation {
    let pid = Pid::from_u32(pid);
    // sysinfo0.39 caches a process's initial BSD process_status while updating
    // its task RSS. A fresh per-PID sample keeps exit status and RSS together;
    // otherwise a process first seen live may retain that status after exit.
    let system = fresh_process(pid, true);
    let Some(process) = system.process(pid) else {
        // Preserve the existing absent-process behavior: only the owned Child
        // can supply an exit result; an observation never creates success.
        return MemoryObservation::Continue;
    };
    let memory = process.memory();
    if memory == 0 && process.status() != ProcessStatus::Zombie {
        // BSD status is read before task RSS inside sysinfo. Exit may occur
        // between those syscalls, so reread status AFTER a zero-RSS result.
        let status = fresh_process(pid, false);
        return status
            .process(pid)
            .map_or(MemoryObservation::Continue, |process| {
                classify(process.status(), memory, ceiling)
            });
    }
    classify(process.status(), memory, ceiling)
}

fn fresh_process(pid: Pid, memory: bool) -> System {
    let mut system = System::new_with_specifics(RefreshKind::nothing());
    let refresh = if memory {
        ProcessRefreshKind::nothing().with_memory()
    } else {
        ProcessRefreshKind::nothing()
    };
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, refresh);
    system
}
