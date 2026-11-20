//! Region hosts supplied by the game, plus the one gateway pair the capture recorded.
//!
//! `titleSegment`, locale, placement ids, and the numeric region id arrive on
//! the calls the game already makes. They are not constants of this crate.
//! Only the EU production hosts from the written capture are filled in. Other
//! regions, including CN, stay empty until they are observed on their own.
//! An override URL passed by the game replaces the captured host for that
//! side only. Hosts are not paths: nothing in this module is fetched.

/// Battle.net region id on the EU capture (`http_param.region`).
pub const CAPTURED_EU_REGION_ID: i32 = 2;

/// API gateway logged for that EU session.
pub const CAPTURED_EU_GATEWAY: &str = "https://eu.api.blizzard.com";

/// OAuth URL logged for that same EU session.
pub const CAPTURED_EU_OAUTH: &str = "https://oauth.battle.net/sso";

/// Scopes the spec names as enough for a read-only client.
///
/// The server-side requirement is not verified. The capture logged a longer
/// list that includes purchase scopes; those are intentionally absent here,
/// and this crate never sends a scope list.
pub const READ_ONLY_SCOPES: &[&str] = &[
    "commerce.catalog.basic",
    "featuredshop.composition.basic",
    "commerce.virtualcurrency.basic",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSource {
    /// `override_gateway_url` or `override_oauth_url` was non-empty.
    GameOverride,
    /// The EU capture in the spec. Not a guess for any other region.
    ObservedCapture,
    /// No host is recorded for this region, and the game did not pass one.
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointChoice {
    pub base: Option<String>,
    pub source: EndpointSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEndpoints {
    pub region_id: i32,
    pub gateway: EndpointChoice,
    pub oauth: EndpointChoice,
}

struct CapturedHosts {
    region_id: i32,
    gateway: &'static str,
    oauth: &'static str,
}

/// The only row is the EU session in the spec. Do not add a region whose
/// hosts were not logged.
const CAPTURED: &[CapturedHosts] = &[CapturedHosts {
    region_id: CAPTURED_EU_REGION_ID,
    gateway: CAPTURED_EU_GATEWAY,
    oauth: CAPTURED_EU_OAUTH,
}];

pub fn resolve_endpoints(
    region_id: i32,
    gateway_override: Option<&str>,
    oauth_override: Option<&str>,
) -> ResolvedEndpoints {
    let captured = CAPTURED.iter().find(|row| row.region_id == region_id);
    ResolvedEndpoints {
        region_id,
        gateway: choose(gateway_override, captured.map(|row| row.gateway)),
        oauth: choose(oauth_override, captured.map(|row| row.oauth)),
    }
}

fn choose(override_url: Option<&str>, captured: Option<&str>) -> EndpointChoice {
    if let Some(url) = nonempty(override_url) {
        return EndpointChoice {
            base: Some(url.to_string()),
            source: EndpointSource::GameOverride,
        };
    }
    if let Some(url) = captured {
        return EndpointChoice {
            base: Some(url.to_string()),
            source: EndpointSource::ObservedCapture,
        };
    }
    EndpointChoice {
        base: None,
        source: EndpointSource::Unspecified,
    }
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_endpoints, EndpointSource, CAPTURED_EU_GATEWAY, CAPTURED_EU_OAUTH,
        CAPTURED_EU_REGION_ID, READ_ONLY_SCOPES,
    };

    #[test]
    fn only_the_captured_eu_hosts_are_filled_in() {
        let eu = resolve_endpoints(CAPTURED_EU_REGION_ID, None, None);
        assert_eq!(eu.gateway.base.as_deref(), Some(CAPTURED_EU_GATEWAY));
        assert_eq!(eu.gateway.source, EndpointSource::ObservedCapture);
        assert_eq!(eu.oauth.base.as_deref(), Some(CAPTURED_EU_OAUTH));
        assert_eq!(eu.oauth.source, EndpointSource::ObservedCapture);

        // These ids are not labeled. They only prove we do not invent a host
        // for a region the capture did not record. CN is one of those regions;
        // its numeric id is not assumed here.
        for region_id in [0, 1, 3, 4, 5, 42, -1] {
            let resolved = resolve_endpoints(region_id, None, None);
            assert_eq!(resolved.gateway.source, EndpointSource::Unspecified);
            assert_eq!(resolved.oauth.source, EndpointSource::Unspecified);
            assert!(resolved.gateway.base.is_none());
            assert!(resolved.oauth.base.is_none());
        }
    }

    #[test]
    fn game_overrides_replace_only_the_side_that_was_set() {
        let resolved = resolve_endpoints(
            CAPTURED_EU_REGION_ID,
            Some("https://gateway.example.test"),
            Some(""),
        );
        assert_eq!(
            resolved.gateway.base.as_deref(),
            Some("https://gateway.example.test")
        );
        assert_eq!(resolved.gateway.source, EndpointSource::GameOverride);
        assert_eq!(resolved.oauth.base.as_deref(), Some(CAPTURED_EU_OAUTH));

        let other = resolve_endpoints(42, None, Some("https://oauth.example.test"));
        assert!(other.gateway.base.is_none());
        assert_eq!(
            other.oauth.base.as_deref(),
            Some("https://oauth.example.test")
        );
        assert_eq!(other.oauth.source, EndpointSource::GameOverride);
    }

    #[test]
    fn read_only_scope_list_has_no_purchase_scope() {
        assert_eq!(
            READ_ONLY_SCOPES,
            [
                "commerce.catalog.basic",
                "featuredshop.composition.basic",
                "commerce.virtualcurrency.basic",
            ]
        );
        assert!(READ_ONLY_SCOPES
            .iter()
            .all(|scope| !scope.contains("purchase")));
    }
}
