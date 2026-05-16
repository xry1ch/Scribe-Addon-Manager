use crate::local::match_remote::MatchResult;
use crate::local::update_plan::{
    build_update_plan, PlannedActionKind, PlannedAddonAction, UpdatePlan,
};

#[cfg(test)]
use std::fmt;

#[cfg(test)]
use thiserror::Error;

#[cfg(test)]
use crate::install::apply::InstallResult;

#[derive(Debug, Clone)]
pub struct UpdateAllPlan {
    pub display_plan: UpdatePlan,
    pub targets: Vec<PlannedAddonAction>,
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub struct UpdateAllApplied {
    pub target: PlannedAddonAction,
    pub result: InstallResult,
}

#[cfg(test)]
pub trait UpdateAllPipeline {
    type Error;

    fn update_one(&mut self, target: &PlannedAddonAction) -> Result<InstallResult, Self::Error>;
}

#[cfg(test)]
#[derive(Debug, Error)]
#[error("failed to update {local_folder}: {source}")]
pub struct UpdateAllApplyError<E> {
    pub local_folder: String,
    pub source: E,
}

pub fn build_update_all_plan(results: &[MatchResult], include_unknown: bool) -> UpdateAllPlan {
    let display_plan = build_update_plan(results, include_unknown);
    let targets = display_plan
        .actions
        .iter()
        .filter(|action| is_update_all_target(action, include_unknown))
        .cloned()
        .collect();

    UpdateAllPlan {
        display_plan,
        targets,
    }
}

#[cfg(test)]
pub fn apply_update_all<P>(
    plan: &UpdateAllPlan,
    pipeline: &mut P,
) -> Result<Vec<UpdateAllApplied>, UpdateAllApplyError<P::Error>>
where
    P: UpdateAllPipeline,
    P::Error: fmt::Display,
{
    let mut applied = Vec::new();

    for target in &plan.targets {
        let result = pipeline
            .update_one(target)
            .map_err(|source| UpdateAllApplyError {
                local_folder: target.local_folder.clone(),
                source,
            })?;
        applied.push(UpdateAllApplied {
            target: target.clone(),
            result,
        });
    }

    Ok(applied)
}

fn is_update_all_target(action: &PlannedAddonAction, include_unknown: bool) -> bool {
    action.remote_uid.is_some()
        && (matches!(action.kind, PlannedActionKind::WouldUpdate)
            || (include_unknown
                && matches!(action.kind, PlannedActionKind::WouldSkipUnknownVersion)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::install::apply::InstallResult;
    use crate::install::update_all::{apply_update_all, build_update_all_plan, UpdateAllPipeline};
    use crate::local::match_remote::{MatchResult, MatchStatus, RemoteCandidate};
    use crate::local::update_plan::PlannedActionKind;
    use crate::local::LocalAddon;

    fn local(folder_name: &str) -> LocalAddon {
        LocalAddon {
            folder_name: folder_name.to_owned(),
            folder_path: PathBuf::from(folder_name),
            manifest_path: None,
            title: Some(folder_name.to_owned()),
            addon_version: Some("1".to_owned()),
            version: None,
            api_versions: Vec::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            saved_variables: Vec::new(),
            saved_variables_per_character: Vec::new(),
            is_library: None,
            author: None,
            description: None,
            valid_manifest: true,
        }
    }

    fn result(folder_name: &str, status: MatchStatus) -> MatchResult {
        let remote = if matches!(
            status,
            MatchStatus::PossibleUpdate
                | MatchStatus::Matched
                | MatchStatus::LocalNewer
                | MatchStatus::UnknownUpdate
        ) {
            Some(RemoteCandidate {
                uid: Some(format!("{folder_name}-uid")),
                name: Some(folder_name.to_owned()),
                author_name: None,
                version: Some("2".to_owned()),
                updated: None,
                file_info_url: None,
                summary: None,
                directories: Vec::new(),
                category_id: None,
                category_name: None,
                downloads: None,
                monthly_downloads: None,
                image_urls: Vec::new(),
                thumbnail_urls: Vec::new(),
                tier: 1,
                score: 100,
                reason: "test".to_owned(),
            })
        } else {
            None
        };

        MatchResult {
            local: local(folder_name),
            status,
            remote,
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }
    }

    #[test]
    fn only_possible_update_is_included_by_default() {
        let plan = build_update_all_plan(
            &[
                result("Update", MatchStatus::PossibleUpdate),
                result("Unknown", MatchStatus::UnknownUpdate),
            ],
            false,
        );

        assert_eq!(plan.targets.len(), 1);
        assert_eq!(plan.targets[0].local_folder, "Update");
        assert_eq!(plan.targets[0].kind, PlannedActionKind::WouldUpdate);
    }

    #[test]
    fn empty_input_returns_empty_update_all_plan() {
        let plan = build_update_all_plan(&[], false);

        assert!(plan.display_plan.actions.is_empty());
        assert!(plan.targets.is_empty());
    }

    #[test]
    fn include_unknown_includes_unknown_update_candidates() {
        let plan = build_update_all_plan(
            &[
                result("Update", MatchStatus::PossibleUpdate),
                result("Unknown", MatchStatus::UnknownUpdate),
            ],
            true,
        );

        assert_eq!(
            plan.targets
                .iter()
                .map(|target| target.local_folder.as_str())
                .collect::<Vec<_>>(),
            vec!["Update", "Unknown"]
        );
    }

    #[test]
    fn current_local_newer_no_match_and_ambiguous_are_excluded() {
        let plan = build_update_all_plan(
            &[
                result("Current", MatchStatus::Matched),
                result("LocalNewer", MatchStatus::LocalNewer),
                result("NoMatch", MatchStatus::NoMatch),
                result("Ambiguous", MatchStatus::Ambiguous),
                result("Library", MatchStatus::Library),
            ],
            true,
        );

        assert!(plan.targets.is_empty());
    }

    #[test]
    fn dry_run_plan_makes_no_pipeline_calls() {
        let plan = build_update_all_plan(&[result("Update", MatchStatus::PossibleUpdate)], false);
        let pipeline = RecordingPipeline::default();

        assert_eq!(plan.targets.len(), 1);
        assert!(pipeline.calls.is_empty());
    }

    #[test]
    fn yes_path_delegates_to_pipeline_for_each_target() {
        let plan = build_update_all_plan(
            &[
                result("One", MatchStatus::PossibleUpdate),
                result("Two", MatchStatus::PossibleUpdate),
            ],
            false,
        );
        let mut pipeline = RecordingPipeline::default();

        let applied = apply_update_all(&plan, &mut pipeline).unwrap();

        assert_eq!(applied.len(), 2);
        assert_eq!(applied[0].target.local_folder, "One");
        assert_eq!(applied[0].result.replaced, 0);
        assert_eq!(pipeline.calls, vec!["One", "Two"]);
    }

    #[test]
    fn failure_stops_and_reports_failed_addon() {
        let plan = build_update_all_plan(
            &[
                result("One", MatchStatus::PossibleUpdate),
                result("Two", MatchStatus::PossibleUpdate),
            ],
            false,
        );
        let mut pipeline = RecordingPipeline {
            fail_on: Some("Two"),
            ..RecordingPipeline::default()
        };

        let error = apply_update_all(&plan, &mut pipeline).unwrap_err();

        assert_eq!(error.local_folder, "Two");
        assert_eq!(pipeline.calls, vec!["One", "Two"]);
    }

    #[derive(Default)]
    struct RecordingPipeline {
        calls: Vec<String>,
        fail_on: Option<&'static str>,
    }

    impl UpdateAllPipeline for RecordingPipeline {
        type Error = &'static str;

        fn update_one(
            &mut self,
            target: &crate::local::update_plan::PlannedAddonAction,
        ) -> Result<InstallResult, Self::Error> {
            self.calls.push(target.local_folder.clone());
            if self.fail_on == Some(target.local_folder.as_str()) {
                return Err("boom");
            }

            Ok(InstallResult::default())
        }
    }
}
