//! In-process transport for read-only commerce calls.
//!
//! [`CommerceTransport::execute`] is one game-facing operation
//! (balance, personalized shop, or product load). The real SDK paged product
//! loads over HTTP and then handed the game a single merged event. Those HTTP
//! calls are not specified, so this crate does not invent follow-up requests.
//! A future gateway implementation may do several reads inside one `execute`
//! and return the merged document. [`NotYetSpecifiedTransport`] does none.
//!
//! Nothing in this module opens a socket.

use crate::model::{BalanceRequest, LoadProductsRequest, PersonalizedShopRequest};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum ReadOnlyCall {
    Balance(BalanceRequest),
    PersonalizedShop(PersonalizedShopRequest),
    LoadProducts(LoadProductsRequest),
}

impl ReadOnlyCall {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Balance(_) => "balance",
            Self::PersonalizedShop(_) => "personalized_shop",
            Self::LoadProducts(_) => "load_products",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExpectedCall {
    BalanceCurrency(String),
    PersonalizedShop,
    LoadProducts,
}

#[derive(Debug)]
pub enum TransportResponse {
    /// Game-facing JSON object. Not an HTTP envelope.
    Fixture(Value),
    /// No path, method, header set, or token exchange is specified for this call.
    NotYetSpecified,
    /// The fixture script refused the call. The body was not a success.
    Rejected { message: String },
}

/// Replay a scripted sequence of game-facing documents.
///
/// This is not an HTTP trace. Steps match the SWIG call, not a URL.
#[derive(Debug, Default)]
pub struct FixtureReplay {
    steps: Vec<FixtureStep>,
    cursor: usize,
    pub calls: Vec<ReadOnlyCall>,
}

#[derive(Debug, Clone)]
pub struct FixtureStep {
    pub expect: ExpectedCall,
    pub body: Value,
}

impl FixtureReplay {
    pub fn new(steps: Vec<FixtureStep>) -> Self {
        Self {
            steps,
            cursor: 0,
            calls: Vec::new(),
        }
    }
}

impl CommerceTransport for FixtureReplay {
    fn execute(&mut self, request: ReadOnlyCall) -> TransportResponse {
        self.calls.push(request.clone());
        let Some(step) = self.steps.get(self.cursor) else {
            return TransportResponse::Rejected {
                message: "fixture script has no step left".to_string(),
            };
        };
        if !step.matches(&request) {
            return TransportResponse::Rejected {
                message: format!(
                    "fixture step {} does not match {}",
                    self.cursor,
                    request.kind_name()
                ),
            };
        }
        self.cursor += 1;
        TransportResponse::Fixture(step.body.clone())
    }
}

impl FixtureStep {
    fn matches(&self, request: &ReadOnlyCall) -> bool {
        match (&self.expect, request) {
            (ExpectedCall::BalanceCurrency(currency), ReadOnlyCall::Balance(body)) => {
                body.currency_code == *currency
            }
            (ExpectedCall::PersonalizedShop, ReadOnlyCall::PersonalizedShop(_)) => true,
            (ExpectedCall::LoadProducts, ReadOnlyCall::LoadProducts(_)) => true,
            _ => false,
        }
    }
}

/// Default transport. `execute` returns immediately and sends nothing.
#[derive(Debug, Default)]
pub struct NotYetSpecifiedTransport;

impl CommerceTransport for NotYetSpecifiedTransport {
    fn execute(&mut self, _request: ReadOnlyCall) -> TransportResponse {
        TransportResponse::NotYetSpecified
    }
}

pub trait CommerceTransport: Send {
    fn execute(&mut self, request: ReadOnlyCall) -> TransportResponse;
}

#[cfg(test)]
mod tests {
    use super::{
        CommerceTransport, ExpectedCall, FixtureReplay, FixtureStep, NotYetSpecifiedTransport,
        TransportResponse,
    };
    use crate::model::{fixtures, BalanceRequest};
    use serde_json::json;

    #[test]
    fn replay_is_ordered_and_a_mismatch_does_not_return_the_body() {
        let mut replay = FixtureReplay::new(vec![
            FixtureStep {
                expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
                body: json!({"ok": true}),
            },
            FixtureStep {
                expect: ExpectedCall::BalanceCurrency("XSR".to_string()),
                body: json!({"ok": false}),
            },
        ]);
        let xsh: BalanceRequest =
            serde_json::from_value(fixtures::balance_request("XSH", "SEG", 0)).unwrap();
        let response = replay.execute(super::ReadOnlyCall::Balance(xsh));
        assert!(matches!(response, TransportResponse::Fixture(_)));

        let wrong: BalanceRequest =
            serde_json::from_value(fixtures::balance_request("XSH", "SEG", 0)).unwrap();
        let response = replay.execute(super::ReadOnlyCall::Balance(wrong));
        assert!(matches!(response, TransportResponse::Rejected { .. }));
        assert_eq!(replay.calls.len(), 2);
    }

    #[test]
    fn unspecified_transport_has_no_body() {
        let mut transport = NotYetSpecifiedTransport;
        let request: BalanceRequest =
            serde_json::from_value(fixtures::balance_request("XSH", "SEG", 0)).unwrap();
        let response = transport.execute(super::ReadOnlyCall::Balance(request));
        assert!(matches!(response, TransportResponse::NotYetSpecified));
    }
}
