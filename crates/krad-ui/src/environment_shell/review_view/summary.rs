use krad_domain::review::{Approvals, CiStatus, Job};
use std::time::Duration;

const SECONDS_PER_MINUTE: u64 = 60;
const MINUTES_PER_HOUR: u64 = 60;
const ISO_MINUTE_LEN: usize = "2026-10-01T10:00".len();
const DATE_TIME_SEPARATOR: char = 'T';
const NAME_SEPARATOR: char = ' ';
const INITIALS_LEN: usize = 2;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct StageSummary {
    pub(super) name: String,
    pub(super) status: CiStatus,
    pub(super) jobs: usize,
}

impl StageSummary {
    pub(super) fn meta(&self) -> String {
        match (self.status, self.jobs) {
            (CiStatus::Manual, _) => "manual".to_owned(),
            (_, 1) => "1 job".to_owned(),
            (_, jobs) => format!("{jobs} jobs"),
        }
    }
}

pub(super) fn stages(jobs: &[Job]) -> Vec<StageSummary> {
    let mut ordered: Vec<&Job> = jobs.iter().collect();
    ordered.sort_by_key(|job| job.id.0);
    let mut stages: Vec<(String, Vec<CiStatus>)> = Vec::new();
    for job in ordered {
        match stages.iter_mut().find(|(name, _)| *name == job.stage) {
            Some((_, statuses)) => statuses.push(job.status),
            None => stages.push((job.stage.clone(), vec![job.status])),
        }
    }
    stages
        .into_iter()
        .map(|(name, statuses)| StageSummary {
            name,
            status: stage_status(&statuses),
            jobs: statuses.len(),
        })
        .collect()
}

fn stage_status(statuses: &[CiStatus]) -> CiStatus {
    let any = |wanted: &[CiStatus]| statuses.iter().any(|status| wanted.contains(status));
    let all = |wanted: CiStatus| statuses.iter().all(|status| *status == wanted);
    if any(&[CiStatus::Failed]) {
        CiStatus::Failed
    } else if any(&[
        CiStatus::Running,
        CiStatus::Pending,
        CiStatus::Preparing,
        CiStatus::WaitingForResource,
        CiStatus::Created,
        CiStatus::Canceling,
    ]) {
        CiStatus::Running
    } else if all(CiStatus::Manual) {
        CiStatus::Manual
    } else if all(CiStatus::Skipped) {
        CiStatus::Skipped
    } else if any(&[CiStatus::Canceled]) {
        CiStatus::Canceled
    } else {
        CiStatus::Success
    }
}

pub(super) fn approval_tally(approvals: &Approvals) -> (usize, usize) {
    let given = approvals.approved_by.len();
    let left = usize::try_from(approvals.approvals_left).unwrap_or(usize::MAX);
    let required = usize::try_from(approvals.approvals_required).unwrap_or(usize::MAX);
    (given, required.max(given.saturating_add(left)))
}

pub(super) fn run_time(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let minutes = seconds / SECONDS_PER_MINUTE;
    let hours = minutes / MINUTES_PER_HOUR;
    if hours > 0 {
        format!("{hours}h {}m", minutes % MINUTES_PER_HOUR)
    } else if minutes > 0 {
        format!("{minutes}m {}s", seconds % SECONDS_PER_MINUTE)
    } else {
        format!("{seconds}s")
    }
}

pub(super) fn short_time(iso: &str) -> String {
    iso.get(..ISO_MINUTE_LEN)
        .unwrap_or(iso)
        .replace(DATE_TIME_SEPARATOR, " ")
}

pub(super) fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(NAME_SEPARATOR)
        .filter(|w| !w.is_empty())
        .collect();
    let letters: String = match words.as_slice() {
        [first, second, ..] => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .collect(),
        [only] => only.chars().take(INITIALS_LEN).collect(),
        [] => String::new(),
    };
    letters.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use krad_domain::review::{Approval, JobId, User, UserId};

    fn job(id: u64, stage: &str, status: CiStatus) -> Job {
        Job {
            id: JobId(id),
            name: format!("job-{id}"),
            stage: stage.to_owned(),
            status,
            web_url: String::new(),
            duration: None,
        }
    }

    #[test]
    fn stages_follow_their_first_job_and_fold_their_statuses() {
        let jobs = [
            job(9, "deploy", CiStatus::Manual),
            job(5, "test", CiStatus::Success),
            job(6, "test", CiStatus::Failed),
            job(7, "security", CiStatus::Running),
            job(1, "build", CiStatus::Success),
            job(4, "test", CiStatus::Skipped),
        ];
        let summary: Vec<(String, CiStatus, String)> = stages(&jobs)
            .into_iter()
            .map(|stage| (stage.name.clone(), stage.status, stage.meta()))
            .collect();
        assert_eq!(
            summary,
            [
                ("build".to_owned(), CiStatus::Success, "1 job".to_owned()),
                ("test".to_owned(), CiStatus::Failed, "3 jobs".to_owned()),
                ("security".to_owned(), CiStatus::Running, "1 job".to_owned()),
                ("deploy".to_owned(), CiStatus::Manual, "manual".to_owned()),
            ]
        );
    }

    #[test]
    fn approvals_count_against_the_larger_of_rule_and_given() {
        let approval = |id| Approval {
            user: User {
                id: UserId(id),
                username: format!("user{id}"),
                name: format!("User {id}"),
            },
        };
        let tally = |given, left, required| {
            approval_tally(&Approvals {
                approved: false,
                approvals_required: required,
                approvals_left: left,
                approved_by: (0..given).map(approval).collect(),
            })
        };
        assert_eq!(tally(1, 1, 2), (1, 2));
        assert_eq!(tally(2, 0, 2), (2, 2));
        assert_eq!(tally(1, 0, 0), (1, 1));
        assert_eq!(tally(0, 0, 0), (0, 0));
    }

    #[test]
    fn run_times_show_the_two_largest_units() {
        assert_eq!(run_time(Duration::from_secs(45)), "45s");
        assert_eq!(run_time(Duration::from_secs(252)), "4m 12s");
        assert_eq!(run_time(Duration::from_mins(63)), "1h 3m");
    }

    #[test]
    fn avatars_and_times_read_from_gitlab_strings() {
        assert_eq!(initials("Marta Ruiz"), "MR");
        assert_eq!(initials("ana"), "AN");
        assert_eq!(initials(" "), "");
        assert_eq!(short_time("2026-10-01T10:00:00.000Z"), "2026-10-01 10:00");
        assert_eq!(short_time("soon"), "soon");
    }
}
