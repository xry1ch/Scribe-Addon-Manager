use crate::api::models::AddonDetails;
use crate::local::match_remote::{MatchResult, MatchStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedActionKind {
    WouldUpdate,
    WouldSkipCurrent,
    WouldSkipLocalNewer,
    WouldSkipUnknownVersion,
    WouldSkipNoMatch,
    WouldSkipAmbiguous,
    WouldSkipLibrary,
}

impl PlannedActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WouldUpdate => "would-update",
            Self::WouldSkipCurrent => "would-skip-current",
            Self::WouldSkipLocalNewer => "would-skip-local-newer",
            Self::WouldSkipUnknownVersion => "would-skip-unknown-version",
            Self::WouldSkipNoMatch => "would-skip-no-match",
            Self::WouldSkipAmbiguous => "would-skip-ambiguous",
            Self::WouldSkipLibrary => "would-skip-library",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlannedAddonDetails {
    pub file_name: Option<String>,
    pub md5: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlannedAddonAction {
    pub local_folder: String,
    pub remote_name: Option<String>,
    pub remote_uid: Option<String>,
    pub local_version: Option<String>,
    pub remote_version: Option<String>,
    pub kind: PlannedActionKind,
    pub details: Option<PlannedAddonDetails>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdatePlan {
    pub actions: Vec<PlannedAddonAction>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdatePlanSummary {
    pub would_update: usize,
    pub current_skipped: usize,
    pub local_newer: usize,
    pub unknown: usize,
    pub no_match: usize,
    pub ambiguous: usize,
    pub libraries: usize,
}

impl UpdatePlan {
    pub fn attach_details(&mut self, uid: &str, details: AddonDetails) {
        for action in &mut self.actions {
            if action.remote_uid.as_deref() == Some(uid) {
                action.details = Some(PlannedAddonDetails {
                    file_name: details.file_name.clone(),
                    md5: details.md5.clone(),
                    download_url: details.download_url.clone(),
                });
            }
        }
    }

    pub fn summary(&self) -> UpdatePlanSummary {
        let mut summary = UpdatePlanSummary::default();

        for action in &self.actions {
            match action.kind {
                PlannedActionKind::WouldUpdate => summary.would_update += 1,
                PlannedActionKind::WouldSkipCurrent => summary.current_skipped += 1,
                PlannedActionKind::WouldSkipLocalNewer => summary.local_newer += 1,
                PlannedActionKind::WouldSkipUnknownVersion => summary.unknown += 1,
                PlannedActionKind::WouldSkipNoMatch => summary.no_match += 1,
                PlannedActionKind::WouldSkipAmbiguous => summary.ambiguous += 1,
                PlannedActionKind::WouldSkipLibrary => summary.libraries += 1,
            }
        }

        summary
    }
}

pub fn build_update_plan(results: &[MatchResult], _include_unknown: bool) -> UpdatePlan {
    UpdatePlan {
        actions: results.iter().map(planned_action).collect(),
    }
}

fn planned_action(result: &MatchResult) -> PlannedAddonAction {
    let local_version = result
        .local
        .addon_version
        .clone()
        .or_else(|| result.local.version.clone());
    let remote_name = result
        .remote
        .as_ref()
        .and_then(|remote| remote.name.clone());
    let remote_uid = result.remote.as_ref().and_then(|remote| remote.uid.clone());
    let remote_version = result
        .remote
        .as_ref()
        .and_then(|remote| remote.version.clone());
    let kind = match result.status {
        MatchStatus::PossibleUpdate => PlannedActionKind::WouldUpdate,
        MatchStatus::Matched => PlannedActionKind::WouldSkipCurrent,
        MatchStatus::LocalNewer => PlannedActionKind::WouldSkipLocalNewer,
        MatchStatus::UnknownUpdate => PlannedActionKind::WouldSkipUnknownVersion,
        MatchStatus::NoMatch => PlannedActionKind::WouldSkipNoMatch,
        MatchStatus::Ambiguous => PlannedActionKind::WouldSkipAmbiguous,
        MatchStatus::Library => PlannedActionKind::WouldSkipLibrary,
    };

    PlannedAddonAction {
        local_folder: result.local.folder_name.clone(),
        remote_name,
        remote_uid,
        local_version,
        remote_version,
        kind,
        details: None,
    }
}

pub fn detail_request_uids_for(results: &[MatchResult], include_unknown: bool) -> Vec<String> {
    results
        .iter()
        .filter(|result| {
            matches!(result.status, MatchStatus::PossibleUpdate)
                || (include_unknown && matches!(result.status, MatchStatus::UnknownUpdate))
        })
        .filter_map(|result| {
            result
                .remote
                .as_ref()
                .and_then(|remote| remote.uid.as_ref())
                .cloned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::local::match_remote::{MatchResult, MatchStatus, RemoteCandidate};
    use crate::local::update_plan::{
        build_update_plan, detail_request_uids_for, PlannedActionKind,
    };
    use crate::local::LocalAddon;

    fn local() -> LocalAddon {
        LocalAddon {
            folder_name: "Addon".to_owned(),
            folder_path: PathBuf::from("Addon"),
            manifest_path: None,
            title: Some("Addon".to_owned()),
            addon_version: Some("1".to_owned()),
            version: None,
            api_versions: Vec::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            is_library: None,
            author: None,
            description: None,
            valid_manifest: true,
        }
    }

    fn matched(status: MatchStatus) -> MatchResult {
        MatchResult {
            local: local(),
            status,
            remote: Some(RemoteCandidate {
                uid: Some("42".to_owned()),
                name: Some("Addon".to_owned()),
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
                tier: 1,
                score: 100,
                reason: "test".to_owned(),
            }),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }
    }

    #[test]
    fn possible_update_becomes_would_update() {
        let plan = build_update_plan(&[matched(MatchStatus::PossibleUpdate)], false);

        assert_eq!(plan.actions[0].kind, PlannedActionKind::WouldUpdate);
        assert_eq!(
            detail_request_uids_for(&[matched(MatchStatus::PossibleUpdate)], false),
            vec!["42"]
        );
    }

    #[test]
    fn matched_current_becomes_would_skip_current() {
        let plan = build_update_plan(&[matched(MatchStatus::Matched)], false);

        assert_eq!(plan.actions[0].kind, PlannedActionKind::WouldSkipCurrent);
        assert!(detail_request_uids_for(&[matched(MatchStatus::Matched)], false).is_empty());
    }

    #[test]
    fn local_newer_becomes_would_skip_local_newer() {
        let plan = build_update_plan(&[matched(MatchStatus::LocalNewer)], false);

        assert_eq!(plan.actions[0].kind, PlannedActionKind::WouldSkipLocalNewer);
        assert!(detail_request_uids_for(&[matched(MatchStatus::LocalNewer)], true).is_empty());
    }

    #[test]
    fn unknown_update_skipped_unless_include_unknown() {
        let plan = build_update_plan(&[matched(MatchStatus::UnknownUpdate)], false);

        assert_eq!(
            plan.actions[0].kind,
            PlannedActionKind::WouldSkipUnknownVersion
        );
        assert!(detail_request_uids_for(&[matched(MatchStatus::UnknownUpdate)], false).is_empty());
    }

    #[test]
    fn unknown_update_included_when_include_unknown() {
        assert_eq!(
            detail_request_uids_for(&[matched(MatchStatus::UnknownUpdate)], true),
            vec!["42"]
        );
    }

    #[test]
    fn no_match_does_not_fetch_details() {
        let result = MatchResult {
            local: local(),
            status: MatchStatus::NoMatch,
            remote: None,
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        };

        assert!(detail_request_uids_for(&[result], true).is_empty());
    }

    #[test]
    fn ambiguous_does_not_fetch_details() {
        let result = MatchResult {
            local: local(),
            status: MatchStatus::Ambiguous,
            remote: None,
            candidates: vec![RemoteCandidate {
                uid: Some("42".to_owned()),
                name: Some("Addon".to_owned()),
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
                tier: 1,
                score: 100,
                reason: "test".to_owned(),
            }],
            debug_candidates: Vec::new(),
        };

        assert!(detail_request_uids_for(&[result], true).is_empty());
    }
}
