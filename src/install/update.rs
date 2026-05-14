use thiserror::Error;

use crate::local::match_remote::{MatchResult, MatchStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateDecision {
    WouldUpdate,
    SkippedCurrent,
    SkippedLocalNewer,
    SkippedUnknownUseForce,
    SkippedNoMatch,
    SkippedAmbiguous,
    ForcedReinstall,
}

impl UpdateDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WouldUpdate => "would-update",
            Self::SkippedCurrent => "skipped-current",
            Self::SkippedLocalNewer => "skipped-local-newer",
            Self::SkippedUnknownUseForce => "skipped-unknown-use-force",
            Self::SkippedNoMatch => "skipped-no-match",
            Self::SkippedAmbiguous => "skipped-ambiguous",
            Self::ForcedReinstall => "forced-reinstall",
        }
    }

    pub fn should_install(&self) -> bool {
        matches!(self, Self::WouldUpdate | Self::ForcedReinstall)
    }
}

#[derive(Debug, Error)]
pub enum UpdateResolveError {
    #[error("no installed addon matched {0}")]
    NoInstalledMatch(String),

    #[error("multiple installed addons matched {0}: {1}")]
    AmbiguousInstalledMatch(String, String),
}

pub fn resolve_update_request<'a>(
    matches: &'a [MatchResult],
    request: &str,
) -> Result<&'a MatchResult, UpdateResolveError> {
    let request = request.trim();
    let candidates = if is_numeric(request) {
        matches
            .iter()
            .filter(|result| {
                result
                    .remote
                    .as_ref()
                    .and_then(|remote| remote.uid.as_deref())
                    == Some(request)
                    || result
                        .candidates
                        .iter()
                        .any(|candidate| candidate.uid.as_deref() == Some(request))
            })
            .collect::<Vec<_>>()
    } else {
        matches
            .iter()
            .filter(|result| result.local.folder_name.eq_ignore_ascii_case(request))
            .collect::<Vec<_>>()
    };

    match candidates.as_slice() {
        [match_result] => Ok(*match_result),
        [] => Err(UpdateResolveError::NoInstalledMatch(request.to_owned())),
        many => Err(UpdateResolveError::AmbiguousInstalledMatch(
            request.to_owned(),
            many.iter()
                .map(|result| result.local.folder_name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        )),
    }
}

pub fn update_decision(result: &MatchResult, force: bool) -> UpdateDecision {
    match result.status {
        MatchStatus::PossibleUpdate => UpdateDecision::WouldUpdate,
        MatchStatus::Matched if force => UpdateDecision::ForcedReinstall,
        MatchStatus::Matched => UpdateDecision::SkippedCurrent,
        MatchStatus::LocalNewer if force => UpdateDecision::ForcedReinstall,
        MatchStatus::LocalNewer => UpdateDecision::SkippedLocalNewer,
        MatchStatus::UnknownUpdate if force => UpdateDecision::ForcedReinstall,
        MatchStatus::UnknownUpdate => UpdateDecision::SkippedUnknownUseForce,
        MatchStatus::NoMatch | MatchStatus::Library => UpdateDecision::SkippedNoMatch,
        MatchStatus::Ambiguous => UpdateDecision::SkippedAmbiguous,
    }
}

fn is_numeric(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::local::match_remote::{MatchResult, MatchStatus, RemoteCandidate};
    use crate::local::LocalAddon;

    use super::{resolve_update_request, update_decision, UpdateDecision};

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
            is_library: None,
            author: None,
            description: None,
            valid_manifest: true,
        }
    }

    fn result(folder_name: &str, uid: Option<&str>, status: MatchStatus) -> MatchResult {
        MatchResult {
            local: local(folder_name),
            status,
            remote: uid.map(|uid| RemoteCandidate {
                uid: Some(uid.to_owned()),
                name: Some(folder_name.to_owned()),
                version: Some("2".to_owned()),
                updated: None,
                tier: 1,
                score: 100,
                reason: "test".to_owned(),
            }),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }
    }

    #[test]
    fn update_by_local_folder_resolves_one_addon() {
        let matches = vec![result("LibAddonMenu-2.0", Some("7"), MatchStatus::Matched)];

        let resolved = resolve_update_request(&matches, "libaddonmenu-2.0").unwrap();

        assert_eq!(resolved.local.folder_name, "LibAddonMenu-2.0");
    }

    #[test]
    fn update_by_uid_resolves_one_addon() {
        let matches = vec![result("LibAddonMenu-2.0", Some("7"), MatchStatus::Matched)];

        let resolved = resolve_update_request(&matches, "7").unwrap();

        assert_eq!(resolved.local.folder_name, "LibAddonMenu-2.0");
    }

    #[test]
    fn no_match_errors_clearly() {
        let error = resolve_update_request(&[], "missing").unwrap_err();

        assert!(error.to_string().contains("no installed addon matched"));
    }

    #[test]
    fn ambiguous_match_refuses() {
        let matches = vec![
            result("One", Some("7"), MatchStatus::Matched),
            result("Two", Some("7"), MatchStatus::Matched),
        ];

        let error = resolve_update_request(&matches, "7").unwrap_err();

        assert!(error
            .to_string()
            .contains("multiple installed addons matched"));
    }

    #[test]
    fn current_skips_unless_force() {
        let matched = result("Addon", Some("1"), MatchStatus::Matched);

        assert_eq!(
            update_decision(&matched, false),
            UpdateDecision::SkippedCurrent
        );
        assert_eq!(
            update_decision(&matched, true),
            UpdateDecision::ForcedReinstall
        );
    }

    #[test]
    fn unknown_skips_unless_force() {
        let matched = result("Addon", Some("1"), MatchStatus::UnknownUpdate);

        assert_eq!(
            update_decision(&matched, false),
            UpdateDecision::SkippedUnknownUseForce
        );
        assert_eq!(
            update_decision(&matched, true),
            UpdateDecision::ForcedReinstall
        );
    }

    #[test]
    fn local_newer_skips_unless_force() {
        let matched = result("Addon", Some("1"), MatchStatus::LocalNewer);

        assert_eq!(
            update_decision(&matched, false),
            UpdateDecision::SkippedLocalNewer
        );
        assert_eq!(
            update_decision(&matched, true),
            UpdateDecision::ForcedReinstall
        );
    }

    #[test]
    fn possible_update_delegates_to_install_path() {
        let matched = result("Addon", Some("1"), MatchStatus::PossibleUpdate);
        let decision = update_decision(&matched, false);

        assert_eq!(decision, UpdateDecision::WouldUpdate);
        assert!(decision.should_install());
    }

    #[test]
    fn force_allows_reinstall_path() {
        let matched = result("Addon", Some("1"), MatchStatus::Matched);
        let decision = update_decision(&matched, true);

        assert_eq!(decision, UpdateDecision::ForcedReinstall);
        assert!(decision.should_install());
    }
}
