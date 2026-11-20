//! Read-only commerce session for the Linux client.
//!
//! This crate is not a Blizzard SDK and it does not contain Blizzard code.
//! It implements the JSON documents and the request lifecycle described in
//! `docs/commerce-spec.md`. The default transport performs no network I/O.
//! Purchase calls are refused in-process from [`REFUSED_EXPORTS`]. HTTP paths,
//! methods, headers, token exchange, gold prices, and error documents are left
//! unspecified.

#![forbid(unsafe_code)]

mod decimal;
mod model;
mod refused;
mod region;
mod session;
mod transport;

pub use decimal::{displayed_balance, DecimalError};
pub use model::{
    fixtures, BalanceRequest, BalanceResponse, LoadProductsRequest, LoadProductsResponse,
    PersonalizedShopRequest, PersonalizedShopResponse,
};
pub use refused::{
    is_refused_export, EXPORT_CHECKOUT_BATTLENET_PURCHASE, EXPORT_CHECKOUT_PURCHASE,
    EXPORT_COMMERCE_VC_PURCHASE, REFUSED_EXPORTS,
};
pub use region::{
    resolve_endpoints, EndpointChoice, EndpointSource, ResolvedEndpoints, CAPTURED_EU_GATEWAY,
    CAPTURED_EU_OAUTH, CAPTURED_EU_REGION_ID, READ_ONLY_SCOPES,
};
pub use session::{DeliveredEvent, InitInput, SdkSession, SessionStats, SessionView};
pub use transport::{
    CommerceTransport, ExpectedCall, FixtureReplay, FixtureStep, NotYetSpecifiedTransport,
    ReadOnlyCall, TransportResponse,
};
