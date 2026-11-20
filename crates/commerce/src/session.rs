//! One SDK session: init parameters, a queue of read-only calls, and events.
//!
//! Requests return as soon as they are queued. [`SdkSession::poll`] is the
//! stand-in for one `blz_commerce_update`. It runs only the calls that were
//! already queued, then returns. A listener that enqueues another call does
//! not get that answer until a later poll. That matches the capture: the shop
//! event is what causes the product load, and the product event arrives on
//! its own update.
//!
//! Purchase methods do not touch the transport. [`SdkSession::resume`] records
//! a successful local no-op, which is what the spec allows for
//! `blz_checkout_resume`.

use crate::model::{
    error_is_unspecified, BalanceRequest, BalanceResponse, LoadProductsRequest,
    LoadProductsResponse, PersonalizedShopRequest, PersonalizedShopResponse,
};
use crate::region::{resolve_endpoints, ResolvedEndpoints};
use crate::transport::{
    CommerceTransport, NotYetSpecifiedTransport, ReadOnlyCall, TransportResponse,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::VecDeque;
use std::fmt;

const UNSPECIFIED_FAULT: &str = "read-only HTTP exchange is not specified; no request was sent";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionStats {
    pub transport_executions: u32,
    pub unspecified_responses: u32,
    pub rejected_responses: u32,
    pub parse_errors: u32,
    pub events_emitted: u32,
    pub purchase_refusals: u32,
    pub resume_calls: u32,
    pub terminated: bool,
}

#[derive(Clone, Default)]
pub struct InitInput {
    pub title_code: String,
    pub title_version: String,
    pub region_id: i32,
    pub environment: i32,
    pub token: String,
    pub client_id: String,
    pub gateway_override: Option<String>,
    pub oauth_override: Option<String>,
    pub locale: String,
    pub game_service_region: String,
    pub device_id: String,
    pub supports_legacy_external_purchases: bool,
}

impl fmt::Debug for InitInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InitInput")
            .field("title_code", &self.title_code)
            .field("title_version", &self.title_version)
            .field("region_id", &self.region_id)
            .field("environment", &self.environment)
            .field("token", &redacted(&self.token))
            .field("client_id", &redacted(&self.client_id))
            .field("gateway_override", &self.gateway_override)
            .field("oauth_override", &self.oauth_override)
            .field("locale", &self.locale)
            .field("game_service_region", &self.game_service_region)
            .field("device_id", &redacted(&self.device_id))
            .field(
                "supports_legacy_external_purchases",
                &self.supports_legacy_external_purchases,
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    pub modules: Vec<String>,
    pub http_client: i64,
    pub region_id: Option<i32>,
    pub environment: Option<i32>,
    pub locale: String,
    pub title_code: String,
    pub title_version: String,
    pub game_service_region: String,
    pub token_present: bool,
    pub client_id_present: bool,
    pub device_id_present: bool,
    pub supports_legacy_external_purchases: bool,
    pub endpoints: Option<ResolvedEndpoints>,
    pub listener_present: bool,
    pub stats: SessionStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveredEvent {
    Balance(String),
    PersonalizedShop(String),
    ProductLoad(String),
}

pub struct SdkSession {
    transport: Box<dyn CommerceTransport>,
    pending: VecDeque<ReadOnlyCall>,
    requests: Vec<ReadOnlyCall>,
    modules: Vec<String>,
    http_client: i64,
    listener_owner: i64,
    listener: Option<usize>,
    init: Option<InitState>,
    stats: SessionStats,
    last_fault: Option<String>,
    terminated: bool,
}

struct InitState {
    title_code: String,
    title_version: String,
    region_id: i32,
    environment: i32,
    token: Secret,
    client_id: Secret,
    locale: String,
    game_service_region: String,
    device_id: Secret,
    supports_legacy_external_purchases: bool,
    endpoints: ResolvedEndpoints,
}

struct Secret(String);

impl Secret {
    fn present(&self) -> bool {
        !self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(redacted(&self.0))
    }
}

impl fmt::Debug for SdkSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SdkSession")
            .field("view", &self.view())
            .field("init", &self.init)
            .finish()
    }
}

impl fmt::Debug for InitState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InitState")
            .field("title_code", &self.title_code)
            .field("region_id", &self.region_id)
            .field("locale", &self.locale)
            .field("token", &self.token)
            .field("client_id", &self.client_id)
            .field("device_id", &self.device_id)
            .field("endpoints", &self.endpoints)
            .finish()
    }
}

fn redacted(value: &str) -> &'static str {
    if value.is_empty() {
        "<empty>"
    } else {
        "<redacted>"
    }
}

impl SdkSession {
    pub fn new(transport: Box<dyn CommerceTransport>) -> Self {
        Self {
            transport,
            pending: VecDeque::new(),
            requests: Vec::new(),
            modules: Vec::new(),
            http_client: 0,
            listener_owner: 0,
            listener: None,
            init: None,
            stats: SessionStats::default(),
            last_fault: None,
            terminated: false,
        }
    }

    pub fn set_transport(&mut self, transport: Box<dyn CommerceTransport>) {
        self.transport = transport;
    }

    pub fn init(&mut self, input: InitInput) {
        let endpoints = resolve_endpoints(
            input.region_id,
            input.gateway_override.as_deref(),
            input.oauth_override.as_deref(),
        );
        self.init = Some(InitState {
            title_code: input.title_code,
            title_version: input.title_version,
            region_id: input.region_id,
            environment: input.environment,
            token: Secret(input.token),
            client_id: Secret(input.client_id),
            locale: input.locale,
            game_service_region: input.game_service_region,
            device_id: Secret(input.device_id),
            supports_legacy_external_purchases: input.supports_legacy_external_purchases,
            endpoints,
        });
    }

    pub fn register_module(&mut self, name: impl Into<String>) {
        self.modules.push(name.into());
    }

    pub fn set_http_client(&mut self, handle: i64) {
        self.http_client = handle;
    }

    pub fn set_listener(&mut self, owner: i64, callback: usize) {
        self.listener_owner = owner;
        self.listener = if callback == 0 { None } else { Some(callback) };
    }

    pub fn listener(&self) -> Option<(i64, usize)> {
        self.listener
            .map(|callback| (self.listener_owner, callback))
    }

    pub fn enqueue_balance(&mut self, json: &str) {
        self.enqueue(json, |request: BalanceRequest| {
            ReadOnlyCall::Balance(request)
        });
    }

    pub fn enqueue_shop(&mut self, json: &str) {
        self.enqueue(json, |request: PersonalizedShopRequest| {
            ReadOnlyCall::PersonalizedShop(request)
        });
    }

    pub fn enqueue_products(&mut self, json: &str) {
        self.enqueue(json, |request: LoadProductsRequest| {
            ReadOnlyCall::LoadProducts(request)
        });
    }

    /// Local refusal. Does not read the transport and does not queue a call.
    pub fn refuse_purchase(&mut self) {
        self.stats.purchase_refusals = self.stats.purchase_refusals.saturating_add(1);
    }

    /// Local success. The capture logs "Resuming purchases"; a read-only
    /// session records the call and does not ask the transport to finish one.
    pub fn resume(&mut self) {
        self.stats.resume_calls = self.stats.resume_calls.saturating_add(1);
    }

    pub fn terminate(&mut self) {
        self.terminated = true;
        self.pending.clear();
        self.listener = None;
    }

    pub fn poll(&mut self) -> Vec<DeliveredEvent> {
        if self.terminated {
            return Vec::new();
        }
        let batch: Vec<_> = self.pending.drain(..).collect();
        let mut delivered = Vec::new();
        for call in batch {
            if let Some(event) = self.dispatch(call) {
                self.stats.events_emitted = self.stats.events_emitted.saturating_add(1);
                delivered.push(event);
            }
        }
        delivered
    }

    pub fn requests(&self) -> &[ReadOnlyCall] {
        &self.requests
    }

    pub fn last_fault(&self) -> Option<&str> {
        self.last_fault.as_deref()
    }

    pub fn view(&self) -> SessionView {
        let init = self.init.as_ref();
        let mut stats = self.stats;
        stats.terminated = self.terminated;
        SessionView {
            modules: self.modules.clone(),
            http_client: self.http_client,
            region_id: init.map(|state| state.region_id),
            environment: init.map(|state| state.environment),
            locale: init.map(|state| state.locale.clone()).unwrap_or_default(),
            title_code: init
                .map(|state| state.title_code.clone())
                .unwrap_or_default(),
            title_version: init
                .map(|state| state.title_version.clone())
                .unwrap_or_default(),
            game_service_region: init
                .map(|state| state.game_service_region.clone())
                .unwrap_or_default(),
            token_present: init.is_some_and(|state| state.token.present()),
            client_id_present: init.is_some_and(|state| state.client_id.present()),
            device_id_present: init.is_some_and(|state| state.device_id.present()),
            supports_legacy_external_purchases: init
                .is_some_and(|state| state.supports_legacy_external_purchases),
            endpoints: init.map(|state| state.endpoints.clone()),
            listener_present: self.listener.is_some(),
            stats,
        }
    }

    fn enqueue<T, F>(&mut self, json: &str, map: F)
    where
        T: DeserializeOwned,
        F: FnOnce(T) -> ReadOnlyCall,
    {
        if self.terminated {
            return;
        }
        match serde_json::from_str::<T>(json) {
            Ok(request) => self.pending.push_back(map(request)),
            Err(error) => {
                self.stats.parse_errors = self.stats.parse_errors.saturating_add(1);
                self.last_fault = Some(error.to_string());
            }
        }
    }

    fn dispatch(&mut self, call: ReadOnlyCall) -> Option<DeliveredEvent> {
        self.requests.push(call.clone());
        self.stats.transport_executions = self.stats.transport_executions.saturating_add(1);
        let response = self.transport.execute(call);
        match response {
            TransportResponse::NotYetSpecified => {
                self.stats.unspecified_responses =
                    self.stats.unspecified_responses.saturating_add(1);
                self.last_fault = Some(UNSPECIFIED_FAULT.to_string());
                None
            }
            TransportResponse::Rejected { message } => {
                self.note_rejection(message);
                None
            }
            TransportResponse::Fixture(value) => self.accept_fixture(&value),
        }
    }

    fn accept_fixture(&mut self, value: &Value) -> Option<DeliveredEvent> {
        let accepted = match self.requests.last() {
            Some(ReadOnlyCall::Balance(_)) => {
                accept::<BalanceResponse, _>(value, |body| error_is_unspecified(&body.error))
                    .map(DeliveredEvent::Balance)
            }
            Some(ReadOnlyCall::PersonalizedShop(_)) => {
                accept::<PersonalizedShopResponse, _>(value, |body| {
                    error_is_unspecified(&body.error)
                })
                .map(DeliveredEvent::PersonalizedShop)
            }
            Some(ReadOnlyCall::LoadProducts(_)) => {
                accept::<LoadProductsResponse, _>(value, |body| error_is_unspecified(&body.error))
                    .map(DeliveredEvent::ProductLoad)
            }
            None => Err("missing request".to_string()),
        };
        match accepted {
            Ok(event) => Some(event),
            Err(message) => {
                self.note_rejection(message);
                None
            }
        }
    }

    fn note_rejection(&mut self, message: String) {
        self.stats.rejected_responses = self.stats.rejected_responses.saturating_add(1);
        self.last_fault = Some(message);
    }
}

fn accept<T, F>(value: &Value, unspecified_error: F) -> Result<String, String>
where
    T: DeserializeOwned,
    F: FnOnce(&T) -> bool,
{
    let parsed: T = serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
    if unspecified_error(&parsed) {
        return Err("error field is not null; the error shape is not specified".to_string());
    }
    serde_json::to_string(value).map_err(|error| error.to_string())
}

impl Default for SdkSession {
    fn default() -> Self {
        Self::new(Box::new(NotYetSpecifiedTransport))
    }
}

#[cfg(test)]
mod tests {
    use super::{InitInput, SdkSession};
    use crate::model::fixtures;
    use crate::region::{EndpointSource, CAPTURED_EU_GATEWAY, CAPTURED_EU_REGION_ID};
    use crate::transport::{
        CommerceTransport, ExpectedCall, FixtureReplay, FixtureStep, ReadOnlyCall,
        TransportResponse,
    };
    use crate::DeliveredEvent;
    use serde_json::json;

    struct PanicTransport;

    impl CommerceTransport for PanicTransport {
        fn execute(&mut self, _request: ReadOnlyCall) -> TransportResponse {
            panic!("purchase and resume must not reach the transport");
        }
    }

    fn session_with(steps: Vec<FixtureStep>) -> SdkSession {
        SdkSession::new(Box::new(FixtureReplay::new(steps)))
    }

    fn init_caller(session: &mut SdkSession, region_id: i32, locale: &str) {
        session.init(InitInput {
            title_code: "WTCG".to_string(),
            title_version: "37.0.0.0".to_string(),
            region_id,
            environment: 2,
            token: "sso-token-not-for-logs".to_string(),
            client_id: "client-id-not-for-logs".to_string(),
            gateway_override: None,
            oauth_override: None,
            locale: locale.to_string(),
            game_service_region: region_id.to_string(),
            device_id: "device-id-not-for-logs".to_string(),
            supports_legacy_external_purchases: true,
        });
    }

    #[test]
    fn init_keeps_the_callers_region_and_redacts_secrets() {
        let mut session = SdkSession::default();
        init_caller(&mut session, 42, "zhCN");
        let view = session.view();
        assert_eq!(view.region_id, Some(42));
        assert_eq!(view.locale, "zhCN");
        assert_eq!(view.title_version, "37.0.0.0");
        assert_eq!(view.game_service_region, "42");
        assert!(view.token_present);
        assert!(view.supports_legacy_external_purchases);
        let endpoints = view.endpoints.expect("endpoints");
        assert_eq!(endpoints.gateway.source, EndpointSource::Unspecified);
        assert!(endpoints.gateway.base.is_none());
        let rendered = format!("{session:?}");
        assert!(!rendered.contains("sso-token-not-for-logs"));
        assert!(!rendered.contains("client-id-not-for-logs"));
        assert!(!rendered.contains("device-id-not-for-logs"));
        assert!(rendered.contains("<redacted>"));
    }

    #[test]
    fn captured_eu_region_resolves_only_when_the_game_says_so() {
        let mut session = SdkSession::default();
        init_caller(&mut session, CAPTURED_EU_REGION_ID, "fr-FR");
        let endpoints = session.view().endpoints.expect("endpoints");
        assert_eq!(endpoints.gateway.base.as_deref(), Some(CAPTURED_EU_GATEWAY));
        assert_eq!(endpoints.gateway.source, EndpointSource::ObservedCapture);
    }

    #[test]
    fn xsh_and_xsr_are_delivered_on_poll_without_rewriting_the_request() {
        let mut session = session_with(vec![
            FixtureStep {
                expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
                body: fixtures::balance("XSH", Some("SEGMENT-FROM-CALLER"), 7, "1", "2"),
            },
            FixtureStep {
                expect: ExpectedCall::BalanceCurrency("XSR".to_string()),
                body: fixtures::balance("XSR", None, 7, "0", "100"),
            },
        ]);
        let xsh = fixtures::balance_request("XSH", "SEGMENT-FROM-CALLER", 9);
        let xsr = fixtures::balance_request("XSR", "", 9);
        session.enqueue_balance(&xsh.to_string());
        session.enqueue_balance(&xsr.to_string());
        assert!(session.view().stats.events_emitted == 0);
        let events = session.poll();
        assert_eq!(events.len(), 2);
        match &events[0] {
            DeliveredEvent::Balance(json) => {
                let value: serde_json::Value = serde_json::from_str(json).unwrap();
                assert_eq!(value["balance"]["ledgerId"]["currencyCode"], "XSH");
                assert_eq!(
                    value["balance"]["ledgerId"]["ledgerKey"]["titleSegment"],
                    "SEGMENT-FROM-CALLER"
                );
                assert_eq!(value["balance"]["purchasedBalance"], "1");
                assert_eq!(value["balance"]["grantedBalance"], "2");
            }
            other => panic!("expected balance, got {other:?}"),
        }
        match &events[1] {
            DeliveredEvent::Balance(json) => {
                let value: serde_json::Value = serde_json::from_str(json).unwrap();
                assert_eq!(value["balance"]["ledgerId"]["currencyCode"], "XSR");
                assert!(value["balance"]["ledgerId"]["ledgerKey"].is_null());
            }
            other => panic!("expected balance, got {other:?}"),
        }
        match &session.requests()[0] {
            ReadOnlyCall::Balance(request) => {
                assert_eq!(request.ledger_key.title_segment, "SEGMENT-FROM-CALLER");
                assert_eq!(request.ledger_key.game_service_region_id, 9);
            }
            other => panic!("expected balance request, got {other:?}"),
        }
        assert_eq!(session.view().stats.transport_executions, 2);
    }

    #[test]
    fn shop_and_catalog_use_the_request_locale_and_do_not_page() {
        let shop = fixtures::personalized_shop("placement-from-caller", "zhCN", 99, "tab_id", "1");
        let mut shop_with_extra = shop.clone();
        shop_with_extra
            .as_object_mut()
            .unwrap()
            .insert("futureField".into(), json!("keep"));
        let mut products = fixtures::load_products(
            "zhCN",
            vec![
                fixtures::priced_product(99, "zhCN", "EUR", "54.99"),
                fixtures::grant_product(100, "zhCN", "XSR", "100"),
            ],
        );
        products["paginationToken"] = json!("300");
        let mut session = session_with(vec![
            FixtureStep {
                expect: ExpectedCall::PersonalizedShop,
                body: shop_with_extra,
            },
            FixtureStep {
                expect: ExpectedCall::LoadProducts,
                body: products,
            },
        ]);
        let region = 42;
        session.enqueue_shop(
            &fixtures::shop_request(region, "zhCN", &["placement-from-caller"]).to_string(),
        );
        let first = session.poll();
        assert_eq!(first.len(), 1);
        match &first[0] {
            DeliveredEvent::PersonalizedShop(json) => {
                let value: serde_json::Value = serde_json::from_str(json).unwrap();
                assert_eq!(
                    value["placements"][0]["placementId"],
                    "placement-from-caller"
                );
                assert_eq!(
                    value["placements"][0]["page"]["sections"][0]["localization"]["locale"],
                    "zhCN"
                );
                assert_eq!(value["futureField"], "keep");
            }
            other => panic!("expected shop, got {other:?}"),
        }
        let ids: Vec<i64> = (0..400).collect();
        session.enqueue_products(&fixtures::products_request(region, "zhCN", &ids).to_string());
        let second = session.poll();
        assert_eq!(second.len(), 1);
        match &second[0] {
            DeliveredEvent::ProductLoad(json) => {
                let value: serde_json::Value = serde_json::from_str(json).unwrap();
                assert_eq!(value["paginationToken"], "300");
                assert_eq!(value["products"][0]["prices"][0]["currencyCode"], "EUR");
                assert_eq!(value["products"][0]["prices"][0]["currentPrice"], "54.99");
                assert!(value["products"][1]["prices"]
                    .as_array()
                    .unwrap()
                    .is_empty());
                assert_eq!(
                    value["products"][1]["virtualCurrencyGrants"][0]["currencyCode"],
                    "XSR"
                );
                let currencies = value["products"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|product| product["prices"].as_array().unwrap())
                    .map(|price| price["currencyCode"].as_str().unwrap().to_string())
                    .collect::<Vec<_>>();
                assert!(!currencies.iter().any(|code| code == "XSH"));
            }
            other => panic!("expected products, got {other:?}"),
        }
        match &session.requests()[0] {
            ReadOnlyCall::PersonalizedShop(request) => {
                assert_eq!(request.locale, "zhCN");
                assert_eq!(request.game_service_region_id, 42);
                assert_eq!(request.placement_ids, ["placement-from-caller"]);
            }
            other => panic!("expected shop request, got {other:?}"),
        }
        match &session.requests()[1] {
            ReadOnlyCall::LoadProducts(request) => {
                assert_eq!(request.locale, "zhCN");
                assert_eq!(request.pagination_size, 300);
                assert_eq!(request.product_ids.len(), 400);
                assert_eq!(request.pagination_token, "");
            }
            other => panic!("expected product request, got {other:?}"),
        }
        // A non-null pagination token does not cause another execution.
        assert_eq!(session.view().stats.transport_executions, 2);
        assert!(session.poll().is_empty());
        assert_eq!(session.view().stats.transport_executions, 2);
    }

    #[test]
    fn a_malformed_fixture_is_not_delivered() {
        let mut session = session_with(vec![FixtureStep {
            expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
            body: json!({"error": null, "balance": {}}),
        }]);
        session.enqueue_balance(&fixtures::balance_request("XSH", "SEG", 0).to_string());
        assert!(session.poll().is_empty());
        let stats = session.view().stats;
        assert_eq!(stats.rejected_responses, 1);
        assert_eq!(stats.events_emitted, 0);
        assert_eq!(stats.transport_executions, 1);
    }

    #[test]
    fn unknown_error_object_is_not_delivered_as_success() {
        let mut body = fixtures::balance("XSH", Some("SEG"), 1, "0", "1");
        body["error"] = json!({"reason": "not-observed"});
        let mut session = session_with(vec![FixtureStep {
            expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
            body,
        }]);
        session.enqueue_balance(&fixtures::balance_request("XSH", "SEG", 0).to_string());
        assert!(session.poll().is_empty());
        assert_eq!(session.view().stats.rejected_responses, 1);
        assert_eq!(
            session.last_fault(),
            Some("error field is not null; the error shape is not specified")
        );
    }

    #[test]
    fn purchases_and_resume_do_not_call_the_transport() {
        let mut session = SdkSession::new(Box::new(PanicTransport));
        session.refuse_purchase();
        session.refuse_purchase();
        session.refuse_purchase();
        session.resume();
        assert!(session.poll().is_empty());
        let stats = session.view().stats;
        assert_eq!(stats.purchase_refusals, 3);
        assert_eq!(stats.resume_calls, 1);
        assert_eq!(stats.transport_executions, 0);
        assert!(session.requests().is_empty());
    }

    #[test]
    fn default_transport_records_the_call_and_emits_nothing() {
        let mut session = SdkSession::default();
        session.enqueue_balance(&fixtures::balance_request("XSH", "SEG", 0).to_string());
        assert!(session.poll().is_empty());
        let stats = session.view().stats;
        assert_eq!(stats.transport_executions, 1);
        assert_eq!(stats.unspecified_responses, 1);
        assert_eq!(stats.events_emitted, 0);
        assert_eq!(
            session.last_fault(),
            Some("read-only HTTP exchange is not specified; no request was sent")
        );
    }

    #[test]
    fn invalid_json_never_reaches_the_transport() {
        let mut session = SdkSession::new(Box::new(PanicTransport));
        session.enqueue_balance("{");
        session.enqueue_shop("[]");
        session.enqueue_products("");
        assert!(session.poll().is_empty());
        assert_eq!(session.view().stats.parse_errors, 3);
        assert_eq!(session.view().stats.transport_executions, 0);
    }

    #[test]
    fn terminate_drops_queued_work_and_later_calls() {
        let mut session = session_with(vec![FixtureStep {
            expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
            body: fixtures::balance("XSH", Some("SEG"), 1, "0", "1"),
        }]);
        session.enqueue_balance(&fixtures::balance_request("XSH", "SEG", 0).to_string());
        session.terminate();
        assert!(session.poll().is_empty());
        session.enqueue_balance(&fixtures::balance_request("XSH", "SEG", 0).to_string());
        assert!(session.poll().is_empty());
        let stats = session.view().stats;
        assert!(stats.terminated);
        assert_eq!(stats.transport_executions, 0);
        assert_eq!(stats.events_emitted, 0);
        assert!(session.listener().is_none());
    }
}
