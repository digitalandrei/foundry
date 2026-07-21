//! Agent protocol-version gates used before the controller queues wire
//! variants that an older agent cannot deserialize.

pub const OPERATIONAL_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 59, 0);
/// Deployment requires the NVIDIA-container readiness probe introduced in
/// 0.63. Operational commands intentionally keep the lower gate so an older
/// or Docker-broken host can still be repaired and upgraded remotely.
pub const DEPLOYMENT_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 63, 0);
/// Stable-name replacement handoff was added in 0.64. An older agent ignores
/// the additive payload field and would safely roll back, but could never
/// create the same-name successor while the predecessor is retained.
pub const REPLACEMENT_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 64, 0);

pub fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let version = version.trim().trim_start_matches('v');
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

pub fn supports(version: Option<&str>, minimum: (u32, u32, u32)) -> bool {
    version
        .and_then(parse)
        .is_some_and(|version| version >= minimum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_and_prefixed_versions() {
        assert_eq!(parse("0.59.0"), Some((0, 59, 0)));
        assert_eq!(parse("v1.2.3-dev"), Some((1, 2, 3)));
        assert_eq!(parse("0.59"), None);
        assert_eq!(parse("unknown"), None);
        assert!(!supports(Some("0.63.9"), REPLACEMENT_MIN_AGENT_VERSION));
        assert!(supports(Some("0.64.0"), REPLACEMENT_MIN_AGENT_VERSION));
    }
}
