#![allow(non_snake_case)]

use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    sync::{
        atomic::{AtomicIsize, Ordering},
        Mutex, OnceLock,
    },
};

const NULL_HANDLE: isize = 0;
const FAKE_SDK_HANDLE: isize = 1;
const RESULT_OK: i32 = 0;
const EVENT_CATALOG_PRODUCTS: isize = 0x1001;
const EVENT_CATALOG_PERSONALIZED_SHOP: isize = 0x1002;
const DATA_CATALOG_PRODUCTS: isize = 0x2001;
const DATA_CATALOG_PERSONALIZED_SHOP: isize = 0x2002;
const DATA_PRODUCT_LOAD: isize = 0x3001;
const DATA_PERSONALIZED_SHOP: isize = 0x3002;
const HTTP_RESULT_OK: isize = 0x4001;

static LISTENER_CALLBACK: AtomicIsize = AtomicIsize::new(0);
static PERSONALIZED_SHOP_RESPONSE: OnceLock<Mutex<String>> = OnceLock::new();

fn leaked_c_string(value: &str) -> *const c_char {
    CString::new(value).unwrap_or_default().into_raw()
}

fn personalized_shop_response() -> &'static Mutex<String> {
    PERSONALIZED_SHOP_RESPONSE.get_or_init(|| Mutex::new(default_personalized_shop_response()))
}

fn default_personalized_shop_response() -> String {
    r#"{"placements":[],"error":null}"#.to_string()
}

fn request_string(request: *const c_char) -> String {
    if request.is_null() {
        return String::new();
    }

    unsafe { CStr::from_ptr(request) }
        .to_string_lossy()
        .into_owned()
}

fn extract_placement_ids(request: &str) -> Vec<String> {
    let Some(key_start) = request.find("\"placementIds\"") else {
        return Vec::new();
    };
    let Some(array_start) = request[key_start..]
        .find('[')
        .map(|offset| key_start + offset + 1)
    else {
        return Vec::new();
    };
    let Some(array_end) = request[array_start..]
        .find(']')
        .map(|offset| array_start + offset)
    else {
        return Vec::new();
    };

    request[array_start..array_end]
        .split(',')
        .filter_map(|item| {
            let item = item.trim().trim_matches('"');
            (!item.is_empty()).then(|| item.to_string())
        })
        .collect()
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            ch => vec![ch],
        })
        .collect()
}

fn build_personalized_shop_response(request: *const c_char) -> String {
    let ids = extract_placement_ids(&request_string(request));
    if ids.is_empty() {
        return default_personalized_shop_response();
    }

    let placements = ids
        .into_iter()
        .map(|id| {
            let id = json_escape(&id);
            format!(
                r#"{{"placementId":"{id}","nextScheduledPageDisplayTimeMs":0,"page":{{"name":"{id}","pageId":"{id}","attributes":[],"sections":[]}}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    format!(r#"{{"placements":[{placements}],"error":null}}"#)
}

fn notify_listener(event: isize) {
    let callback = LISTENER_CALLBACK.load(Ordering::SeqCst);
    if callback == 0 {
        return;
    }

    let callback: extern "C" fn(isize, isize) = unsafe { std::mem::transmute(callback) };
    callback(NULL_HANDLE, event);
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_create___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_get___(
    _result: isize,
) -> isize {
    FAKE_SDK_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_get___(
    _result: isize,
) -> i32 {
    RESULT_OK
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_set___(
    _result: isize,
    _state: i32,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_set___(
    _result: isize,
    _sdk: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_create_result_t___(
    _result: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_t___(_sdk: isize) {}

#[no_mangle]
pub extern "C" fn SWIGRegisterStringCallback_battlenet_commerce(_callback: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_init___(
    _sdk: isize,
    _init_pairs: isize,
    _count: i32,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_update___(_sdk: isize) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_terminate___(_sdk: isize) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register___(
    _sdk: isize,
    _manifest: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_log___(
    _owner: isize,
    _hook: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_unregister_log___(_owner: isize) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_add_listener___(
    _sdk: isize,
    _owner: isize,
    listener: isize,
) -> isize {
    LISTENER_CALLBACK.store(listener, Ordering::SeqCst);
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_register_blz_http___(
    _sdk: isize,
    _http_client: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_catalog___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_checkout___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_http___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_scene___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_vc___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_catalog_load_products___(
    _sdk: isize,
    _request: *const c_char,
) -> isize {
    notify_listener(EVENT_CATALOG_PRODUCTS);
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___(
    _sdk: isize,
    request: *const c_char,
) -> isize {
    if let Ok(mut response) = personalized_shop_response().lock() {
        *response = build_personalized_shop_response(request);
    }
    notify_listener(EVENT_CATALOG_PERSONALIZED_SHOP);
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_purchase___(
    _sdk: isize,
    _purchase: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___(
    _sdk: isize,
    _purchase: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_cancel_purchase___(
    _sdk: isize,
    _cancel: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_resume___(_sdk: isize) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(
    _sdk: isize,
    _request: *const c_char,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___(
    _sdk: isize,
    _request: *const c_char,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_browser_send_event___(
    _sdk: isize,
    _event_type: i32,
    _data: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_generate_transaction_id___() -> *const c_char
{
    leaked_c_string("unsupported")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_new_blz_commerce_result_t___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___(
    _result: isize,
) -> i32 {
    RESULT_OK
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_result_t_state_set___(
    _result: isize,
    _state: i32,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_result_t_reference_id_get___(
    _result: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_result_t_reference_id_set___(
    _result: isize,
    _reference_id: i32,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_result_t_data_get___(
    _result: isize,
) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_result_t_data_set___(
    _result: isize,
    _data: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_result_t___(_result: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_new_blzCommercePairArray___(_size: i32) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blzCommercePairArray___(_array: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blzCommercePairArray_setitem___(
    _array: isize,
    _index: i32,
    _value: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_new_blz_commerce_pair_t___() -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_pair_t___(_pair: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_pair_t_key_get___(
    _pair: isize,
) -> *const c_char {
    leaked_c_string("")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_pair_t_key_set___(
    _pair: isize,
    _key: *const c_char,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_pair_t_data_get___(_pair: isize) -> isize {
    NULL_HANDLE
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_pair_t_data_set___(
    _pair: isize,
    _data: isize,
) {
}

macro_rules! string_constant {
    ($symbol:ident, $value:literal) => {
        #[no_mangle]
        pub extern "C" fn $symbol() -> *const c_char {
            leaked_c_string($value)
        }
    };
}

string_constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_HTTP_PARAM_get___,
    "BLZ_COMMERCE_HTTP_PARAM"
);
string_constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_BROWSER_PARAM_get___,
    "BLZ_COMMERCE_BROWSER_PARAM"
);
string_constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_BROWSER_PARAM_get___,
    "BLZ_COMMERCE_CHECKOUT_BROWSER_PARAM"
);
string_constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_PARAM_get___,
    "BLZ_COMMERCE_CHECKOUT_PARAM"
);

macro_rules! new_and_delete {
    ($new_symbol:ident, $delete_symbol:ident) => {
        #[no_mangle]
        pub extern "C" fn $new_symbol() -> isize {
            NULL_HANDLE
        }

        #[no_mangle]
        pub extern "C" fn $delete_symbol(_handle: isize) {}
    };
}

new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_http_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_http_params_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_browser_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_browser_params_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_checkout_browser_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_browser_params_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_checkout_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_params_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_purchase_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_purchase_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_browser_purchase_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_browser_purchase_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_purchase_cancel_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_purchase_cancel_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_vec2d_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_vec2d_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_key_input_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_key_input_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_mouse_input_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_mouse_input_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_character_input_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_character_input_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_mouse_move_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_mouse_move_t___
);
new_and_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_mouse_wheel_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_mouse_wheel_t___
);

macro_rules! no_op_setter {
    ($symbol:ident, $value_ty:ty) => {
        #[no_mangle]
        pub extern "C" fn $symbol(_handle: isize, _value: $value_ty) {}
    };
}

macro_rules! int_getter {
    ($symbol:ident, $value_ty:ty) => {
        #[no_mangle]
        pub extern "C" fn $symbol(_handle: isize) -> $value_ty {
            0 as $value_ty
        }
    };
}

macro_rules! bool_getter {
    ($symbol:ident) => {
        #[no_mangle]
        pub extern "C" fn $symbol(_handle: isize) -> bool {
            false
        }
    };
}

macro_rules! ptr_getter {
    ($symbol:ident) => {
        #[no_mangle]
        pub extern "C" fn $symbol(_handle: isize) -> isize {
            NULL_HANDLE
        }
    };
}

macro_rules! string_getter {
    ($symbol:ident) => {
        #[no_mangle]
        pub extern "C" fn $symbol(_handle: isize) -> *const c_char {
            leaked_c_string("")
        }
    };
}

macro_rules! string_field {
    ($get_symbol:ident, $set_symbol:ident) => {
        string_getter!($get_symbol);
        no_op_setter!($set_symbol, *const c_char);
    };
}

macro_rules! int_field {
    ($get_symbol:ident, $set_symbol:ident, $value_ty:ty) => {
        int_getter!($get_symbol, $value_ty);
        no_op_setter!($set_symbol, $value_ty);
    };
}

macro_rules! bool_field {
    ($get_symbol:ident, $set_symbol:ident) => {
        bool_getter!($get_symbol);
        no_op_setter!($set_symbol, bool);
    };
}

macro_rules! ptr_field {
    ($get_symbol:ident, $set_symbol:ident) => {
        ptr_getter!($get_symbol);
        no_op_setter!($set_symbol, isize);
    };
}

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_client_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_client_id_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_token_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_token_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_code_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_version_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_version_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_oauth_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_oauth_url_set___
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_set___,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_environment_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_environment_set___,
    i32
);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_set___,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_height_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_height_set___,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_width_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_width_set___,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_height_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_height_set___,
    i32
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_log_directory_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_log_directory_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_browser_directory_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_browser_directory_set___
);
bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_is_prod_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_is_prod_set___
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_http_environment_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_http_environment_set___,
    i32
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_override_urls_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_override_urls_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_checkout_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_checkout_url_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_navbar_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_navbar_url_set___
);
bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_use_static_asset_loading_page_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_use_static_asset_loading_page_set___
);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_code_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_device_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_device_id_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_version_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_version_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_service_region_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_service_region_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_account_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_account_id_set___
);
bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_params_t_supports_legacy_external_platform_purchases_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_params_t_supports_legacy_external_platform_purchases_set___
);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_routing_key_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_routing_key_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_server_validation_signature_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_server_validation_signature_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_sso_token_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_sso_token_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_externalTransactionId_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_externalTransactionId_set___
);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_id_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_currency_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_currency_id_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_browser_purchase_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_browser_purchase_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_claims_token_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_claims_token_set___
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_cancel_t_transaction_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_cancel_t_transaction_id_set___
);

ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_config_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_config_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_post_init_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_post_init_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_terminate_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_terminate_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_update_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_update_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_name_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_name_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_scopes_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_scopes_set___
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependencies_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependencies_set___
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependency_count_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependency_count_set___,
    u32
);
#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_manifest_t___(_manifest: isize) {}

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_x_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_x_set___,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_y_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_y_set___,
    i32
);

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_event_t_type_get___(event: isize) -> i32 {
    match event {
        EVENT_CATALOG_PRODUCTS | EVENT_CATALOG_PERSONALIZED_SHOP => 3,
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_event_t_type_set___(
    _event: isize,
    _event_type: i32,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_event_t_data_get___(event: isize) -> isize {
    match event {
        EVENT_CATALOG_PRODUCTS => DATA_CATALOG_PRODUCTS,
        EVENT_CATALOG_PERSONALIZED_SHOP => DATA_CATALOG_PERSONALIZED_SHOP,
        _ => NULL_HANDLE,
    }
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_event_t_data_set___(
    _event: isize,
    _data: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_event_t_reference_id_get___(
    _event: isize,
) -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_event_t_reference_id_set___(
    _event: isize,
    _reference_id: i32,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_event_t___(_event: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_type_get___(
    event: isize,
) -> i32 {
    match event {
        DATA_CATALOG_PRODUCTS => 1,
        DATA_CATALOG_PERSONALIZED_SHOP => 2,
        _ => 0,
    }
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_type_set___(
    _event: isize,
    _catalog_type: i32,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_data_get___(
    event: isize,
) -> isize {
    match event {
        DATA_CATALOG_PRODUCTS => DATA_PRODUCT_LOAD,
        DATA_CATALOG_PERSONALIZED_SHOP => DATA_PERSONALIZED_SHOP,
        _ => NULL_HANDLE,
    }
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_data_set___(
    _event: isize,
    _data: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_catalog_event_t___(_event: isize) {}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_response_get___(
    _event: isize,
) -> *const c_char {
    let response = personalized_shop_response()
        .lock()
        .map(|response| response.clone())
        .unwrap_or_else(|_| default_personalized_shop_response());
    leaked_c_string(&response)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_response_set___(
    _event: isize,
    _response: *const c_char,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_http_result_get___(
    _event: isize,
) -> isize {
    HTTP_RESULT_OK
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_http_result_set___(
    _event: isize,
    _http_result: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_catalog_personalized_shop_event_t___(
    _event: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_response_get___(
    _event: isize,
) -> *const c_char {
    leaked_c_string(r#"{"products":[],"childProducts":[],"error":null}"#)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_response_set___(
    _event: isize,
    _response: *const c_char,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_http_result_get___(
    _event: isize,
) -> isize {
    HTTP_RESULT_OK
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_http_result_set___(
    _event: isize,
    _http_result: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_catalog_product_load_event_t___(
    _event: isize,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_ok_get___(
    event: isize,
) -> bool {
    event == HTTP_RESULT_OK
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_ok_set___(
    _event: isize,
    _ok: bool,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_result_code_get___(
    event: isize,
) -> i64 {
    if event == HTTP_RESULT_OK {
        200
    } else {
        0
    }
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_result_code_set___(
    _event: isize,
    _code: i64,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_system_code_get___(
    _event: isize,
) -> i64 {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_system_code_set___(
    _event: isize,
    _code: i64,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_message_get___(
    _event: isize,
) -> *const c_char {
    leaked_c_string("")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_message_set___(
    _event: isize,
    _message: *const c_char,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_http_enabled_event_t___(
    _event: isize,
) {
}
