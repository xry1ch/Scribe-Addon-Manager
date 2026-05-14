use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionComparison {
    RemoteNewer,
    Same,
    LocalNewer,
    Unknown,
}

pub fn compare_versions(local: Option<&str>, remote: Option<&str>) -> VersionComparison {
    let Some(local) = normalize_present(local) else {
        return VersionComparison::Unknown;
    };
    let Some(remote) = normalize_present(remote) else {
        return VersionComparison::Unknown;
    };

    if let (Some(local), Some(remote)) = (parse_integer(local), parse_integer(remote)) {
        return ordering_to_comparison(local.cmp(&remote));
    }

    if let (Some(local), Some(remote)) = (parse_dotted_numeric(local), parse_dotted_numeric(remote))
    {
        return compare_numeric_segments(&local, &remote);
    }

    let local_release = parse_release_marker(local);
    let remote_release = parse_release_marker(remote);
    match (
        parse_integer(local),
        parse_integer(remote),
        local_release,
        remote_release,
    ) {
        (Some(local), None, None, Some(remote)) => ordering_to_comparison(local.cmp(&remote)),
        (None, Some(remote), Some(local), None) => ordering_to_comparison(local.cmp(&remote)),
        (None, None, Some(local), Some(remote)) => ordering_to_comparison(local.cmp(&remote)),
        _ => VersionComparison::Unknown,
    }
}

fn normalize_present(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    (!value.is_empty()).then_some(value)
}

fn parse_integer(value: &str) -> Option<u64> {
    if value.chars().all(|ch| ch.is_ascii_digit()) {
        value.parse().ok()
    } else {
        None
    }
}

fn parse_dotted_numeric(value: &str) -> Option<Vec<u64>> {
    if !value.contains('.') {
        return None;
    }

    let mut segments = Vec::new();
    for segment in value.split('.') {
        if segment.is_empty() || !segment.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        segments.push(segment.parse().ok()?);
    }

    Some(segments)
}

fn parse_release_marker(value: &str) -> Option<u64> {
    let lower = value.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut releases = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        if starts_with_marker(bytes, index, b"revision") {
            if let Some((release, next)) = parse_release_number(bytes, index + b"revision".len()) {
                releases.push(release);
                index = next;
                continue;
            }
        } else if starts_with_marker(bytes, index, b"rev") {
            if let Some((release, next)) = parse_release_number(bytes, index + b"rev".len()) {
                releases.push(release);
                index = next;
                continue;
            }
        } else if bytes[index] == b'r' && marker_start_allowed(bytes, index) {
            if let Some((release, next)) = parse_release_number(bytes, index + 1) {
                releases.push(release);
                index = next;
                continue;
            }
        }

        index += 1;
    }

    match releases.as_slice() {
        [release] => Some(*release),
        _ => None,
    }
}

fn starts_with_marker(bytes: &[u8], index: usize, marker: &[u8]) -> bool {
    bytes[index..].starts_with(marker)
        && marker_start_allowed(bytes, index)
        && bytes
            .get(index + marker.len())
            .is_some_and(|ch| ch.is_ascii_whitespace() || ch.is_ascii_digit())
}

fn marker_start_allowed(bytes: &[u8], index: usize) -> bool {
    index == 0 || !bytes[index - 1].is_ascii_alphabetic()
}

fn parse_release_number(bytes: &[u8], mut index: usize) -> Option<(u64, usize)> {
    while bytes.get(index).is_some_and(|ch| ch.is_ascii_whitespace()) {
        index += 1;
    }

    let start = index;
    while bytes.get(index).is_some_and(|ch| ch.is_ascii_digit()) {
        index += 1;
    }

    if start == index {
        return None;
    }

    std::str::from_utf8(&bytes[start..index])
        .ok()?
        .parse()
        .ok()
        .map(|release| (release, index))
}

fn compare_numeric_segments(local: &[u64], remote: &[u64]) -> VersionComparison {
    let len = local.len().max(remote.len());

    for index in 0..len {
        let local = local.get(index).copied().unwrap_or(0);
        let remote = remote.get(index).copied().unwrap_or(0);
        match local.cmp(&remote) {
            Ordering::Less => return VersionComparison::RemoteNewer,
            Ordering::Greater => return VersionComparison::LocalNewer,
            Ordering::Equal => {}
        }
    }

    VersionComparison::Same
}

fn ordering_to_comparison(ordering: Ordering) -> VersionComparison {
    match ordering {
        Ordering::Less => VersionComparison::RemoteNewer,
        Ordering::Equal => VersionComparison::Same,
        Ordering::Greater => VersionComparison::LocalNewer,
    }
}

#[cfg(test)]
mod tests {
    use super::{compare_versions, VersionComparison};

    #[test]
    fn addon_version_integer_matches_remote_release_marker() {
        assert_eq!(
            compare_versions(Some("43"), Some("2.0 r43")),
            VersionComparison::Same
        );
    }

    #[test]
    fn lower_integer_is_older_than_remote_release_marker() {
        assert_eq!(
            compare_versions(Some("42"), Some("2.0 r43")),
            VersionComparison::RemoteNewer
        );
    }

    #[test]
    fn higher_integer_is_newer_than_remote_release_marker() {
        assert_eq!(
            compare_versions(Some("44"), Some("2.0 r43")),
            VersionComparison::LocalNewer
        );
    }

    #[test]
    fn equal_dotted_versions_match() {
        assert_eq!(
            compare_versions(Some("1.0.0"), Some("1.0.0")),
            VersionComparison::Same
        );
    }

    #[test]
    fn dotted_versions_compare_segment_by_segment() {
        assert_eq!(
            compare_versions(Some("1.0.0"), Some("1.0.1")),
            VersionComparison::RemoteNewer
        );
    }

    #[test]
    fn missing_dotted_segments_are_zero() {
        assert_eq!(
            compare_versions(Some("1.2"), Some("1.2.0")),
            VersionComparison::Same
        );
    }

    #[test]
    fn release_markers_compare_to_each_other() {
        assert_eq!(
            compare_versions(Some("r5"), Some("r6")),
            VersionComparison::RemoteNewer
        );
    }

    #[test]
    fn dotted_numeric_dates_compare_without_date_inference() {
        assert_eq!(
            compare_versions(Some("2024.01.01"), Some("2024.01.02")),
            VersionComparison::RemoteNewer
        );
    }

    #[test]
    fn words_are_unknown() {
        assert_eq!(
            compare_versions(Some("alpha"), Some("beta")),
            VersionComparison::Unknown
        );
    }

    #[test]
    fn missing_versions_are_unknown() {
        assert_eq!(
            compare_versions(None, Some("1")),
            VersionComparison::Unknown
        );
        assert_eq!(
            compare_versions(Some("1"), None),
            VersionComparison::Unknown
        );
        assert_eq!(
            compare_versions(Some(""), Some("1")),
            VersionComparison::Unknown
        );
    }

    #[test]
    fn compact_release_marker_is_supported() {
        assert_eq!(
            compare_versions(Some("43"), Some("2.0r43")),
            VersionComparison::Same
        );
    }

    #[test]
    fn revision_words_are_supported() {
        assert_eq!(
            compare_versions(Some("rev5"), Some("revision 6")),
            VersionComparison::RemoteNewer
        );
    }
}
