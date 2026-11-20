//! The game's P/Invoke sequence, driven through the exported symbols.
//!
//! Fixtures stay in memory. Nothing in this module is allowed to open a socket.

use super::{
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_BROWSER_PARAM_get___,
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_BROWSER_PARAM_get___,
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_PARAM_get___,
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_HTTP_PARAM_get___,
    CSharp_BlizzardfCommerce_blzCommercePairArray_setitem___,
    CSharp_BlizzardfCommerce_blz_catalog_load_products___,
    CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___,
    CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___,
    CSharp_BlizzardfCommerce_blz_checkout_cancel_purchase___,
    CSharp_BlizzardfCommerce_blz_checkout_purchase___,
    CSharp_BlizzardfCommerce_blz_checkout_resume___,
    CSharp_BlizzardfCommerce_blz_commerce_add_listener___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_is_prod_set___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_set___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_type_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_http_result_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_response_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_http_result_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_response_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_device_id_set___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_service_region_set___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_set___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_params_t_supports_legacy_external_platform_purchases_set___,
    CSharp_BlizzardfCommerce_blz_commerce_create___,
    CSharp_BlizzardfCommerce_blz_commerce_event_t_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_event_t_type_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_ok_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_result_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_client_id_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_environment_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_oauth_url_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_code_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_version_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_token_set___,
    CSharp_BlizzardfCommerce_blz_commerce_http_register_blz_http___,
    CSharp_BlizzardfCommerce_blz_commerce_init___,
    CSharp_BlizzardfCommerce_blz_commerce_pair_t_data_set___,
    CSharp_BlizzardfCommerce_blz_commerce_pair_t_key_set___,
    CSharp_BlizzardfCommerce_blz_commerce_register___,
    CSharp_BlizzardfCommerce_blz_commerce_register_catalog___,
    CSharp_BlizzardfCommerce_blz_commerce_register_checkout___,
    CSharp_BlizzardfCommerce_blz_commerce_register_http___,
    CSharp_BlizzardfCommerce_blz_commerce_register_log___,
    CSharp_BlizzardfCommerce_blz_commerce_register_scene___,
    CSharp_BlizzardfCommerce_blz_commerce_register_vc___,
    CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___,
    CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_get___,
    CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_get___,
    CSharp_BlizzardfCommerce_blz_commerce_terminate___,
    CSharp_BlizzardfCommerce_blz_commerce_unregister_log___,
    CSharp_BlizzardfCommerce_blz_commerce_update___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_type_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_http_result_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_response_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___,
    CSharp_BlizzardfCommerce_delete_blzCommercePairArray___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_browser_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_browser_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_http_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_manifest_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_pair_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_result_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_create_result_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_t___,
    CSharp_BlizzardfCommerce_new_blzCommercePairArray___,
    CSharp_BlizzardfCommerce_new_blz_commerce_browser_params_t___,
    CSharp_BlizzardfCommerce_new_blz_commerce_checkout_browser_params_t___,
    CSharp_BlizzardfCommerce_new_blz_commerce_checkout_params_t___,
    CSharp_BlizzardfCommerce_new_blz_commerce_http_params_t___,
    CSharp_BlizzardfCommerce_new_blz_commerce_pair_t___,
    SWIGRegisterStringCallback_battlenet_commerce,
};
use crate::registry;
use crate::strings;
use hearthstone_linux_gui_commerce::{
    fixtures, CommerceTransport, EndpointSource, ExpectedCall, FixtureReplay, FixtureStep,
    ReadOnlyCall, TransportResponse,
};
use serde_json::Value;
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, MutexGuard};

static HARNESS: Mutex<()> = Mutex::new(());
static CAPTURED: Mutex<Vec<Captured>> = Mutex::new(Vec::new());
static CALLBACK_ARGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static REENTER: AtomicBool = AtomicBool::new(false);
static ACTIVE_SDK: AtomicIsize = AtomicIsize::new(0);
static REENTER_REQUEST: Mutex<String> = Mutex::new(String::new());

struct Captured {
    owner: isize,
    module: i32,
    sub: i32,
    http_ok: bool,
    result_code: i64,
    json: Value,
}

struct Started {
    sdk: isize,
    manifests: Vec<isize>,
}

fn harness() -> MutexGuard<'static, ()> {
    let guard = HARNESS.lock().unwrap_or_else(|error| error.into_inner());
    registry::reset();
    strings::reset();
    CAPTURED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    CALLBACK_ARGS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    REENTER.store(false, Ordering::SeqCst);
    ACTIVE_SDK.store(0, Ordering::SeqCst);
    REENTER_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    guard
}

extern "C" fn copy_string(ptr: *const c_char) -> *mut c_char {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    CALLBACK_ARGS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(text.clone());
    CString::new(text).unwrap_or_default().into_raw()
}

extern "C" fn on_event(owner: isize, event: isize) {
    let captured = capture(owner, event);
    if REENTER.load(Ordering::SeqCst) && captured.module == 3 && captured.sub == 2 {
        let request = reenter_request();
        let c_request = CString::new(request).unwrap_or_default();
        CSharp_BlizzardfCommerce_blz_catalog_load_products___(
            ACTIVE_SDK.load(Ordering::SeqCst),
            c_request.as_ptr(),
        );
    }
    CAPTURED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .push(captured);
}

fn reenter_request() -> String {
    REENTER_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn set_reenter_request(json: String) {
    *REENTER_REQUEST
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = json;
}

fn capture(owner: isize, event: isize) -> Captured {
    let module = CSharp_BlizzardfCommerce_blz_commerce_event_t_type_get___(event);
    let middle = CSharp_BlizzardfCommerce_blz_commerce_event_t_data_get___(event);
    let (sub, specific) = if module == 2 {
        (
            CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_type_get___(middle),
            CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_data_get___(middle),
        )
    } else {
        (
            CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_type_get___(middle),
            CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_data_get___(middle),
        )
    };
    let (json_ptr, http) = if module == 2 {
        (
            CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_response_get___(specific),
            CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_http_result_get___(
                specific,
            ),
        )
    } else if sub == 1 {
        (
            CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_response_get___(
                specific,
            ),
            CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_http_result_get___(
                specific,
            ),
        )
    } else {
        (
            CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_response_get___(
                specific,
            ),
            CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_http_result_get___(
                specific,
            ),
        )
    };
    Captured {
        owner,
        module,
        sub,
        http_ok: CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_ok_get___(http),
        result_code: CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_result_code_get___(
            http,
        ),
        json: parse_json(json_ptr),
    }
}

fn parse_json(ptr: *const c_char) -> Value {
    if ptr.is_null() {
        return Value::Null;
    }
    let text = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

fn c_string(text: &str) -> CString {
    CString::new(text).unwrap_or_default()
}

fn start(region: i32, locale: &str, gateway: Option<&str>) -> Started {
    SWIGRegisterStringCallback_battlenet_commerce(copy_string as usize as isize);
    let created = CSharp_BlizzardfCommerce_blz_commerce_create___();
    assert_ne!(created, 0);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_get___(created),
        0
    );
    let sdk = CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_get___(created);
    assert_ne!(sdk, 0);
    CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_create_result_t___(created);

    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_http_register_blz_http___(sdk, 5),
        0
    );
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_register_log___(9, 0x11),
        0
    );
    let manifests = vec![
        CSharp_BlizzardfCommerce_blz_commerce_register_http___(),
        CSharp_BlizzardfCommerce_blz_commerce_register_scene___(),
        CSharp_BlizzardfCommerce_blz_commerce_register_checkout___(),
        CSharp_BlizzardfCommerce_blz_commerce_register_catalog___(),
        CSharp_BlizzardfCommerce_blz_commerce_register_vc___(),
    ];
    for manifest in &manifests {
        assert_ne!(*manifest, 0);
        assert_eq!(
            CSharp_BlizzardfCommerce_blz_commerce_register___(sdk, *manifest),
            0
        );
    }
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_add_listener___(sdk, 7, on_event as usize as isize),
        0
    );

    let http = CSharp_BlizzardfCommerce_new_blz_commerce_http_params_t___();
    let title = c_string("WTCG");
    let version = c_string("37.0.0.0");
    let token = c_string("sso-token-not-for-logs");
    let client_id = c_string("client-id-not-for-logs");
    let gateway_owned = gateway.map(c_string);
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_code_set___(http, title.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_version_set___(
        http,
        version.as_ptr(),
    );
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_set___(http, region);
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_environment_set___(http, 2);
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_token_set___(http, token.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_client_id_set___(http, client_id.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_set___(
        http,
        gateway_owned
            .as_ref()
            .map(|value| value.as_ptr())
            .unwrap_or(std::ptr::null()),
    );
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_oauth_url_set___(
        http,
        std::ptr::null(),
    );
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_get___(http),
        region
    );
    let gateway_ptr =
        CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_get___(http);
    if gateway.is_none() {
        assert!(gateway_ptr.is_null());
    } else {
        assert!(!gateway_ptr.is_null());
    }

    let browser = CSharp_BlizzardfCommerce_new_blz_commerce_browser_params_t___();
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_is_prod_set___(browser, true);
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_set___(browser, 800);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_get___(browser),
        800
    );

    let checkout_browser = CSharp_BlizzardfCommerce_new_blz_commerce_checkout_browser_params_t___();
    let locale_c = c_string(locale);
    let service_region = c_string(&region.to_string());
    let device = c_string("device-id-not-for-logs");
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_set___(
        checkout_browser,
        locale_c.as_ptr(),
    );
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_service_region_set___(
        checkout_browser,
        service_region.as_ptr(),
    );
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_device_id_set___(
        checkout_browser,
        device.as_ptr(),
    );
    let locale_ptr = CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_get___(
        checkout_browser,
    );
    assert_eq!(
        unsafe { CStr::from_ptr(locale_ptr) }.to_string_lossy(),
        locale
    );

    let checkout = CSharp_BlizzardfCommerce_new_blz_commerce_checkout_params_t___();
    CSharp_BlizzardfCommerce_blz_commerce_checkout_params_t_supports_legacy_external_platform_purchases_set___(
        checkout, true,
    );

    let pairs = CSharp_BlizzardfCommerce_new_blzCommercePairArray___(4);
    let keys = [
        CSharp_BlizzardfCommerce_BLZ_COMMERCE_HTTP_PARAM_get___(),
        CSharp_BlizzardfCommerce_BLZ_COMMERCE_BROWSER_PARAM_get___(),
        CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_BROWSER_PARAM_get___(),
        CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_PARAM_get___(),
    ];
    let datas = [http, browser, checkout_browser, checkout];
    let mut pair_handles = Vec::new();
    for (index, (key, data)) in keys.into_iter().zip(datas).enumerate() {
        let pair = CSharp_BlizzardfCommerce_new_blz_commerce_pair_t___();
        CSharp_BlizzardfCommerce_blz_commerce_pair_t_key_set___(pair, key);
        CSharp_BlizzardfCommerce_blz_commerce_pair_t_data_set___(pair, data);
        CSharp_BlizzardfCommerce_blzCommercePairArray_setitem___(pairs, index as i32, pair);
        pair_handles.push(pair);
    }
    let init_result = CSharp_BlizzardfCommerce_blz_commerce_init___(sdk, pairs, 4);
    assert_ne!(init_result, 0);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___(init_result),
        0
    );
    CSharp_BlizzardfCommerce_delete_blz_commerce_result_t___(init_result);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___(init_result),
        0
    );
    for pair in pair_handles {
        CSharp_BlizzardfCommerce_delete_blz_commerce_pair_t___(pair);
    }
    CSharp_BlizzardfCommerce_delete_blzCommercePairArray___(pairs);
    CSharp_BlizzardfCommerce_delete_blz_commerce_http_params_t___(http);
    CSharp_BlizzardfCommerce_delete_blz_commerce_browser_params_t___(browser);
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_browser_params_t___(checkout_browser);
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_params_t___(checkout);

    Started { sdk, manifests }
}

fn install(sdk: isize, steps: Vec<FixtureStep>) {
    registry::testing_set_transport(sdk, Box::new(FixtureReplay::new(steps)));
}

fn captured() -> Vec<Captured> {
    CAPTURED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .drain(..)
        .collect()
}

fn socket_rows() -> usize {
    [
        "/proc/self/net/tcp",
        "/proc/self/net/tcp6",
        "/proc/self/net/udp",
        "/proc/self/net/udp6",
    ]
    .into_iter()
    .map(|path| {
        std::fs::read_to_string(path)
            .map(|text| text.lines().count().saturating_sub(1))
            .unwrap_or(0)
    })
    .sum()
}

struct PanicTransport;

impl CommerceTransport for PanicTransport {
    fn execute(&mut self, _request: ReadOnlyCall) -> TransportResponse {
        panic!("this call must be refused before the transport");
    }
}

#[test]
fn init_keeps_the_callers_region_and_registers_modules_in_order() {
    let _guard = harness();
    let started = start(42, "zhCN", Some("https://gateway.example.test"));
    let view = registry::testing_view(started.sdk).expect("sdk");
    assert_eq!(view.modules, ["http", "Scene", "Checkout", "Catalog", "vc"]);
    assert_eq!(view.http_client, 5);
    assert_eq!(view.region_id, Some(42));
    assert_eq!(view.environment, Some(2));
    assert_eq!(view.locale, "zhCN");
    assert_eq!(view.title_code, "WTCG");
    assert_eq!(view.title_version, "37.0.0.0");
    assert_eq!(view.game_service_region, "42");
    assert!(view.token_present);
    assert!(view.client_id_present);
    assert!(view.device_id_present);
    assert!(view.supports_legacy_external_purchases);
    assert!(view.listener_present);
    let endpoints = view.endpoints.as_ref().expect("endpoints");
    assert_eq!(
        endpoints.gateway.base.as_deref(),
        Some("https://gateway.example.test")
    );
    assert_eq!(endpoints.gateway.source, EndpointSource::GameOverride);
    assert_eq!(endpoints.oauth.source, EndpointSource::Unspecified);
    assert_eq!(registry::testing_log(), (9, 0x11));
    CSharp_BlizzardfCommerce_blz_commerce_unregister_log___(9);
    assert_eq!(registry::testing_log(), (0, 0));
    let args = CALLBACK_ARGS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert!(args.iter().any(|arg| arg == "BLZ_COMMERCE_HTTP_PARAM"));
    assert!(args.iter().any(|arg| arg == "zhCN"));
    assert!(!format!("{view:?}").contains("sso-token-not-for-logs"));
}

#[test]
fn balance_shop_and_catalog_fixtures_arrive_on_update() {
    let _guard = harness();
    let started = start(42, "zhCN", None);
    let mut products = fixtures::load_products(
        "zhCN",
        vec![
            fixtures::priced_product(99, "zhCN", "EUR", "54.99"),
            fixtures::grant_product(100, "zhCN", "XSR", "100"),
        ],
    );
    products["paginationToken"] = serde_json::json!("300");
    install(
        started.sdk,
        vec![
            FixtureStep {
                expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
                body: fixtures::balance("XSH", Some("SEGMENT-FROM-CALLER"), 7, "1", "2"),
            },
            FixtureStep {
                expect: ExpectedCall::BalanceCurrency("XSR".to_string()),
                body: fixtures::balance("XSR", None, 7, "0", "100"),
            },
            FixtureStep {
                expect: ExpectedCall::PersonalizedShop,
                body: fixtures::personalized_shop(
                    "placement-from-caller",
                    "zhCN",
                    99,
                    "tab_id",
                    "1",
                ),
            },
            FixtureStep {
                expect: ExpectedCall::LoadProducts,
                body: products,
            },
        ],
    );

    let xsh = c_string(&fixtures::balance_request("XSH", "SEGMENT-FROM-CALLER", 9).to_string());
    let xsr = c_string(&fixtures::balance_request("XSR", "", 9).to_string());
    assert_ne!(
        CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(started.sdk, xsh.as_ptr()),
        0
    );
    assert_ne!(
        CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(started.sdk, xsr.as_ptr()),
        0
    );
    assert!(captured().is_empty(), "events wait for update");

    let frame = CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    assert_ne!(frame, 0);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___(frame),
        0
    );
    CSharp_BlizzardfCommerce_delete_blz_commerce_result_t___(frame);
    let events = captured();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|event| event.owner == 7 && event.http_ok));
    assert!(events.iter().all(|event| event.result_code == 0));
    assert_eq!(events[0].module, 2);
    assert_eq!(events[0].sub, 1);
    assert_eq!(events[0].json["balance"]["ledgerId"]["currencyCode"], "XSH");
    assert_eq!(
        events[0].json["balance"]["ledgerId"]["ledgerKey"]["titleSegment"],
        "SEGMENT-FROM-CALLER"
    );
    assert_eq!(events[0].json["balance"]["purchasedBalance"], "1");
    assert_eq!(events[0].json["balance"]["grantedBalance"], "2");
    assert_eq!(events[1].json["balance"]["ledgerId"]["currencyCode"], "XSR");
    assert!(events[1].json["balance"]["ledgerId"]["ledgerKey"].is_null());

    let shop =
        c_string(&fixtures::shop_request(42, "zhCN", &["placement-from-caller"]).to_string());
    CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___(started.sdk, shop.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    let events = captured();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].module, 3);
    assert_eq!(events[0].sub, 2);
    assert_eq!(
        events[0].json["placements"][0]["placementId"],
        "placement-from-caller"
    );
    assert_eq!(
        events[0].json["placements"][0]["page"]["sections"][0]["localization"]["locale"],
        "zhCN"
    );

    let ids: Vec<i64> = (0..400).collect();
    let products = c_string(&fixtures::products_request(42, "zhCN", &ids).to_string());
    CSharp_BlizzardfCommerce_blz_catalog_load_products___(started.sdk, products.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    let events = captured();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].module, 3);
    assert_eq!(events[0].sub, 1);
    assert_eq!(events[0].json["paginationToken"], "300");
    assert_eq!(
        events[0].json["products"][0]["prices"][0]["currencyCode"],
        "EUR"
    );
    assert_eq!(
        events[0].json["products"][0]["prices"][0]["currentPrice"],
        "54.99"
    );
    assert!(events[0].json["products"][1]["prices"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        events[0].json["products"][1]["virtualCurrencyGrants"][0]["currencyCode"],
        "XSR"
    );
    let requests = registry::testing_requests(started.sdk);
    assert_eq!(requests.len(), 4);
    match &requests[3] {
        ReadOnlyCall::LoadProducts(request) => {
            assert_eq!(request.pagination_size, 300);
            assert_eq!(request.product_ids.len(), 400);
            assert_eq!(request.locale, "zhCN");
        }
        other => panic!("expected one product load, got {other:?}"),
    }
    let args = CALLBACK_ARGS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    assert!(args.iter().any(|arg| arg.contains("SEGMENT-FROM-CALLER")));
}

#[test]
fn shop_listener_enqueues_products_for_the_next_update() {
    let _guard = harness();
    let started = start(42, "zhCN", None);
    ACTIVE_SDK.store(started.sdk, Ordering::SeqCst);
    let ids = [99_i64];
    set_reenter_request(fixtures::products_request(42, "zhCN", &ids).to_string());
    REENTER.store(true, Ordering::SeqCst);
    install(
        started.sdk,
        vec![
            FixtureStep {
                expect: ExpectedCall::PersonalizedShop,
                body: fixtures::personalized_shop(
                    "placement-from-caller",
                    "zhCN",
                    99,
                    "tab_id",
                    "1",
                ),
            },
            FixtureStep {
                expect: ExpectedCall::LoadProducts,
                body: fixtures::load_products(
                    "zhCN",
                    vec![fixtures::priced_product(99, "zhCN", "EUR", "1.00")],
                ),
            },
        ],
    );
    let shop =
        c_string(&fixtures::shop_request(42, "zhCN", &["placement-from-caller"]).to_string());
    CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___(started.sdk, shop.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    let first = captured();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].sub, 2);
    assert_eq!(
        registry::testing_view(started.sdk)
            .unwrap()
            .stats
            .transport_executions,
        1
    );
    CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    let second = captured();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].sub, 1);
    assert_eq!(second[0].json["products"][0]["productId"], 99);
    assert_eq!(
        registry::testing_view(started.sdk)
            .unwrap()
            .stats
            .transport_executions,
        2
    );
}

#[test]
fn purchases_are_refused_before_any_transport_call() {
    let _guard = harness();
    let started = start(42, "zhCN", None);
    registry::testing_set_transport(started.sdk, Box::new(PanicTransport));
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_checkout_purchase___(started.sdk, 0),
        0
    );
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___(started.sdk, 0),
        0
    );
    let payload = c_string("{\"productId\":1}");
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___(started.sdk, payload.as_ptr()),
        0
    );
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_checkout_cancel_purchase___(started.sdk, 0),
        0
    );
    let resumed = CSharp_BlizzardfCommerce_blz_checkout_resume___(started.sdk);
    assert_ne!(resumed, 0);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___(resumed),
        0
    );
    assert!(captured().is_empty());
    let stats = registry::testing_view(started.sdk).unwrap().stats;
    assert_eq!(stats.purchase_refusals, 3);
    assert_eq!(stats.resume_calls, 1);
    assert_eq!(stats.transport_executions, 0);

    install(
        started.sdk,
        vec![FixtureStep {
            expect: ExpectedCall::BalanceCurrency("XSH".to_string()),
            body: fixtures::balance("XSH", Some("SEG"), 1, "0", "3"),
        }],
    );
    let request = c_string(&fixtures::balance_request("XSH", "SEG", 0).to_string());
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(started.sdk, request.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    let events = captured();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].json["balance"]["grantedBalance"], "3");
    assert_eq!(registry::testing_requests(started.sdk).len(), 1);
}

#[test]
fn teardown_drops_pending_work_and_a_new_session_can_start() {
    let _guard = harness();
    let started = start(42, "zhCN", None);
    registry::testing_set_transport(started.sdk, Box::new(PanicTransport));
    let request = c_string(&fixtures::balance_request("XSH", "SEG", 0).to_string());
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(started.sdk, request.as_ptr());
    let ended = CSharp_BlizzardfCommerce_blz_commerce_terminate___(started.sdk);
    assert_ne!(ended, 0);
    assert!(CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk) != 0);
    assert!(captured().is_empty());
    assert_eq!(
        registry::testing_view(started.sdk)
            .unwrap()
            .stats
            .transport_executions,
        0
    );
    assert!(
        registry::testing_view(started.sdk)
            .unwrap()
            .stats
            .terminated
    );
    for manifest in started.manifests {
        CSharp_BlizzardfCommerce_delete_blz_commerce_manifest_t___(manifest);
    }
    CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_t___(started.sdk);
    CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_t___(started.sdk);
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk),
        0
    );
    assert_eq!(
        CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(started.sdk, request.as_ptr()),
        0
    );

    let again = start(2, "fr-FR", None);
    let endpoints = registry::testing_view(again.sdk)
        .unwrap()
        .endpoints
        .unwrap();
    assert_eq!(endpoints.region_id, 2);
    assert_eq!(
        endpoints.gateway.base.as_deref(),
        Some("https://eu.api.blizzard.com")
    );
    assert_eq!(
        endpoints.oauth.base.as_deref(),
        Some("https://oauth.battle.net/sso")
    );
    assert_eq!(endpoints.gateway.source, EndpointSource::ObservedCapture);
}

#[test]
fn the_default_transport_opens_no_socket() {
    let _guard = harness();
    assert!(std::path::Path::new("/proc/self/net/tcp").exists());
    let before = socket_rows();
    let started = start(42, "zhCN", None);
    let balance = c_string(&fixtures::balance_request("XSH", "SEG", 0).to_string());
    let shop = c_string(&fixtures::shop_request(42, "zhCN", &["placement"]).to_string());
    let products = c_string(&fixtures::products_request(42, "zhCN", &[1, 2, 3]).to_string());
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(started.sdk, balance.as_ptr());
    CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___(started.sdk, shop.as_ptr());
    CSharp_BlizzardfCommerce_blz_catalog_load_products___(started.sdk, products.as_ptr());
    CSharp_BlizzardfCommerce_blz_commerce_update___(started.sdk);
    CSharp_BlizzardfCommerce_blz_checkout_purchase___(started.sdk, 1);
    CSharp_BlizzardfCommerce_blz_checkout_resume___(started.sdk);
    assert!(captured().is_empty());
    let stats = registry::testing_view(started.sdk).unwrap().stats;
    assert_eq!(stats.unspecified_responses, 3);
    assert_eq!(stats.events_emitted, 0);
    assert_eq!(stats.transport_executions, 3);
    assert_eq!(stats.purchase_refusals, 1);
    assert_eq!(socket_rows(), before);
}
