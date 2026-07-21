//! Agent protocol-version gates used before the controller queues wire
//! variants that an older agent cannot deserialize.

pub const OPERATIONAL_MIN_AGENT_VERSION: (u32, u32, u32) = (0, 59, 0);

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
    }
}
