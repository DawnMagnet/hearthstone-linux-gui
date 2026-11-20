//! Read-only Linux stand-in for `blz_commerce_sdk_plugin`.
//!
//! The game P/Invokes the `CSharp_BlizzardfCommerce_*` symbols below. This
//! library is not a Blizzard SDK, does not load Blizzard binaries, and does
//! not send purchase traffic. The default transport performs no network I/O.
//! See `docs/commerce-impl.md`.

#![allow(non_snake_case)]

mod lifecycle;
mod registry;
mod strings;

#[cfg(test)]
mod tests;

use registry::Field;
use std::os::raw::c_char;

macro_rules! constant {
    ($name:ident, $text:expr) => {
        #[no_mangle]
        pub extern "C" fn $name() -> *const c_char {
            strings::export_static($text)
        }
    };
}

macro_rules! new_delete {
    ($new:ident, $delete:ident) => {
        #[no_mangle]
        pub extern "C" fn $new() -> isize {
            registry::alloc_plain()
        }

        #[no_mangle]
        pub extern "C" fn $delete(handle: isize) {
            registry::free(handle);
        }
    };
}

macro_rules! deleter {
    ($name:ident) => {
        #[no_mangle]
        pub extern "C" fn $name(handle: isize) {
            registry::free(handle);
        }
    };
}

macro_rules! string_field {
    ($get:ident, $set:ident, $field:ident) => {
        #[no_mangle]
        pub extern "C" fn $get(handle: isize) -> *const c_char {
            strings::export_text(registry::text(handle, Field::$field))
        }

        #[no_mangle]
        pub extern "C" fn $set(handle: isize, value: *const c_char) {
            registry::set_text(handle, Field::$field, value);
        }
    };
}

macro_rules! int_field {
    ($get:ident, $set:ident, $field:ident, $ty:ty) => {
        #[no_mangle]
        pub extern "C" fn $get(handle: isize) -> $ty {
            registry::int(handle, Field::$field) as $ty
        }

        #[no_mangle]
        pub extern "C" fn $set(handle: isize, value: $ty) {
            registry::set_int(handle, Field::$field, i64::from(value));
        }
    };
}

macro_rules! bool_field {
    ($get:ident, $set:ident, $field:ident) => {
        #[no_mangle]
        pub extern "C" fn $get(handle: isize) -> bool {
            registry::flag(handle, Field::$field)
        }

        #[no_mangle]
        pub extern "C" fn $set(handle: isize, value: bool) {
            registry::set_flag(handle, Field::$field, value);
        }
    };
}

macro_rules! ptr_field {
    ($get:ident, $set:ident, $field:ident) => {
        #[no_mangle]
        pub extern "C" fn $get(handle: isize) -> isize {
            registry::ptr(handle, Field::$field)
        }

        #[no_mangle]
        pub extern "C" fn $set(handle: isize, value: isize) {
            registry::set_ptr(handle, Field::$field, value);
        }
    };
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_create___() -> isize {
    lifecycle::create()
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_create_result_t___(
    result: isize,
) {
    registry::free(result);
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blz_commerce_sdk_t___(sdk: isize) {
    registry::free(sdk);
}

#[no_mangle]
pub extern "C" fn SWIGRegisterStringCallback_battlenet_commerce(callback: isize) {
    strings::register(callback as usize);
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_init___(
    sdk: isize,
    pairs: isize,
    count: i32,
) -> isize {
    lifecycle::init(sdk, pairs, count)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_update___(sdk: isize) -> isize {
    lifecycle::update(sdk)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_terminate___(sdk: isize) -> isize {
    lifecycle::terminate(sdk)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register___(
    sdk: isize,
    manifest: isize,
) -> isize {
    lifecycle::register_module(sdk, manifest)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_log___(
    owner: isize,
    hook: isize,
) -> isize {
    // The callback signature was not captured. Store the pointer and do not call it.
    registry::set_log(owner, hook);
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_unregister_log___(owner: isize) -> isize {
    registry::clear_log(owner);
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_add_listener___(
    sdk: isize,
    owner: isize,
    listener: isize,
) -> isize {
    lifecycle::add_listener(sdk, owner, listener as usize)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_http_register_blz_http___(
    sdk: isize,
    http_client: isize,
) -> isize {
    lifecycle::register_http_client(sdk, http_client)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_http___() -> isize {
    lifecycle::manifest("http")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_scene___() -> isize {
    lifecycle::manifest("Scene")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_checkout___() -> isize {
    lifecycle::manifest("Checkout")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_catalog___() -> isize {
    lifecycle::manifest("Catalog")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_register_vc___() -> isize {
    lifecycle::manifest("vc")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_catalog_load_products___(
    sdk: isize,
    request: *const c_char,
) -> isize {
    lifecycle::products(sdk, request)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_catalog_personalized_shop___(
    sdk: isize,
    request: *const c_char,
) -> isize {
    lifecycle::shop(sdk, request)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___(
    sdk: isize,
    request: *const c_char,
) -> isize {
    lifecycle::balance(sdk, request)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_purchase___(
    sdk: isize,
    _purchase: isize,
) -> isize {
    lifecycle::refuse_purchase(sdk)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___(
    sdk: isize,
    _purchase: isize,
) -> isize {
    lifecycle::refuse_purchase(sdk)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___(
    sdk: isize,
    _request: *const c_char,
) -> isize {
    lifecycle::refuse_purchase(sdk)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_cancel_purchase___(
    _sdk: isize,
    _cancel: isize,
) -> isize {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_checkout_resume___(sdk: isize) -> isize {
    lifecycle::resume(sdk)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_browser_send_event___(
    _sdk: isize,
    _event_type: i32,
    _data: isize,
) -> isize {
    0
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_generate_transaction_id___() -> *const c_char
{
    strings::export("unsupported")
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_new_blzCommercePairArray___(size: i32) -> isize {
    registry::alloc_list(size)
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_delete_blzCommercePairArray___(array: isize) {
    registry::free(array);
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blzCommercePairArray_setitem___(
    array: isize,
    index: i32,
    value: isize,
) {
    registry::list_set(array, index, value);
}

constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_HTTP_PARAM_get___,
    lifecycle::HTTP_PARAM
);
constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_BROWSER_PARAM_get___,
    lifecycle::BROWSER_PARAM
);
constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_BROWSER_PARAM_get___,
    lifecycle::CHECKOUT_BROWSER_PARAM
);
constant!(
    CSharp_BlizzardfCommerce_BLZ_COMMERCE_CHECKOUT_PARAM_get___,
    lifecycle::CHECKOUT_PARAM
);

// The capture shows `override_urls` as the number 1. The exported setter is
// pointer-sized, matching the previous stub, and must not dereference that value.
#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_override_urls_set___(
    _handle: isize,
    _value: *const c_char,
) {
}

#[no_mangle]
pub extern "C" fn CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_override_urls_get___(
    _handle: isize,
) -> *const c_char {
    strings::export("")
}

new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_result_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_result_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_pair_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_pair_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_http_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_http_params_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_browser_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_browser_params_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_checkout_browser_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_browser_params_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_checkout_params_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_checkout_params_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_purchase_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_purchase_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_browser_purchase_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_browser_purchase_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_purchase_cancel_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_purchase_cancel_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_vec2d_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_vec2d_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_key_input_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_key_input_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_mouse_input_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_mouse_input_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_character_input_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_character_input_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_mouse_move_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_mouse_move_t___
);
new_delete!(
    CSharp_BlizzardfCommerce_new_blz_commerce_mouse_wheel_t___,
    CSharp_BlizzardfCommerce_delete_blz_commerce_mouse_wheel_t___
);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_get___,
    CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_state_set___,
    State,
    i32
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_get___,
    CSharp_BlizzardfCommerce_blz_commerce_sdk_create_result_t_sdk_set___,
    Sdk
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_result_t_state_get___,
    CSharp_BlizzardfCommerce_blz_commerce_result_t_state_set___,
    State,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_result_t_reference_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_result_t_reference_id_set___,
    ReferenceId,
    i32
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_result_t_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_result_t_data_set___,
    Data
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_pair_t_key_get___,
    CSharp_BlizzardfCommerce_blz_commerce_pair_t_key_set___,
    PairKey
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_pair_t_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_pair_t_data_set___,
    Data
);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_client_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_client_id_set___,
    ClientId
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_token_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_token_set___,
    Token
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_code_set___,
    TitleCode
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_version_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_title_version_set___,
    TitleVersion
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_gateway_url_set___,
    OverrideGateway
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_oauth_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_override_oauth_url_set___,
    OverrideOauth
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_region_set___,
    Region,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_environment_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_params_t_environment_set___,
    Environment,
    i32
);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_width_set___,
    WindowWidth,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_height_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_window_height_set___,
    WindowHeight,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_width_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_width_set___,
    MaxWindowWidth,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_height_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_max_window_height_set___,
    MaxWindowHeight,
    i32
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_log_directory_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_log_directory_set___,
    LogDirectory
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_browser_directory_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_browser_directory_set___,
    BrowserDirectory
);
bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_is_prod_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_is_prod_set___,
    IsProd
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_http_environment_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_http_environment_set___,
    HttpEnvironment,
    i32
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_checkout_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_checkout_url_set___,
    CheckoutUrl
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_navbar_url_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_navbar_url_set___,
    NavbarUrl
);
bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_use_static_asset_loading_page_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_params_t_use_static_asset_loading_page_set___,
    UseStaticAssetLoadingPage
);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_code_set___,
    TitleCode
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_device_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_device_id_set___,
    DeviceId
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_version_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_title_version_set___,
    TitleVersion
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_locale_set___,
    Locale
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_service_region_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_service_region_set___,
    GameServiceRegion
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_account_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_browser_params_t_game_account_id_set___,
    GameAccountId
);
bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_checkout_params_t_supports_legacy_external_platform_purchases_get___,
    CSharp_BlizzardfCommerce_blz_commerce_checkout_params_t_supports_legacy_external_platform_purchases_set___,
    SupportsLegacy
);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_routing_key_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_routing_key_set___,
    RoutingKey
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_server_validation_signature_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_server_validation_signature_set___,
    ServerValidationSignature
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_sso_token_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_sso_token_set___,
    SsoToken
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_externalTransactionId_get___,
    CSharp_BlizzardfCommerce_blz_commerce_browser_purchase_t_externalTransactionId_set___,
    ExternalTransactionId
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_id_set___,
    PurchaseProductId
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_currency_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_currency_id_set___,
    PurchaseCurrencyId
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_browser_purchase_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_browser_purchase_set___,
    BrowserPurchase
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_claims_token_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_t_product_claims_token_set___,
    ProductClaimsToken
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_purchase_cancel_t_transaction_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_purchase_cancel_t_transaction_id_set___,
    CancelTransactionId
);

ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_config_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_config_set___,
    ManifestConfig
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_post_init_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_post_init_set___,
    ManifestPostInit
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_terminate_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_terminate_set___,
    ManifestTerminate
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_update_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_update_set___,
    ManifestUpdate
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_name_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_name_set___,
    ManifestGetName
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_scopes_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_get_scopes_set___,
    ManifestGetScopes
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependencies_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependencies_set___,
    ManifestDependencies
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependency_count_get___,
    CSharp_BlizzardfCommerce_blz_commerce_manifest_t_dependency_count_set___,
    DependencyCount,
    u32
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_manifest_t___);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_x_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_x_set___,
    VecX,
    i32
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_y_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vec2d_t_y_set___,
    VecY,
    i32
);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_event_t_type_get___,
    CSharp_BlizzardfCommerce_blz_commerce_event_t_type_set___,
    Type,
    i32
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_event_t_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_event_t_data_set___,
    Data
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_event_t_reference_id_get___,
    CSharp_BlizzardfCommerce_blz_commerce_event_t_reference_id_set___,
    ReferenceId,
    i32
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_event_t___);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_type_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_type_set___,
    CatalogType,
    i32
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_event_t_catalog_data_set___,
    CatalogData
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_catalog_event_t___);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_response_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_response_set___,
    Response
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_http_result_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_personalized_shop_event_t_http_result_set___,
    HttpResult
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_catalog_personalized_shop_event_t___);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_response_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_response_set___,
    Response
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_http_result_get___,
    CSharp_BlizzardfCommerce_blz_commerce_catalog_product_load_event_t_http_result_set___,
    HttpResult
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_catalog_product_load_event_t___);

int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_type_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_type_set___,
    VcType,
    i32
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_data_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_event_t_vc_data_set___,
    VcData
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_vc_event_t___);

string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_response_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_response_set___,
    Response
);
ptr_field!(
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_http_result_get___,
    CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance_event_t_http_result_set___,
    HttpResult
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_vc_get_balance_event_t___);

bool_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_ok_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_ok_set___,
    HttpOk
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_result_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_result_code_set___,
    ResultCode,
    i64
);
int_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_system_code_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_system_code_set___,
    SystemCode,
    i64
);
string_field!(
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_message_get___,
    CSharp_BlizzardfCommerce_blz_commerce_http_enabled_event_t_message_set___,
    HttpMessage
);
deleter!(CSharp_BlizzardfCommerce_delete_blz_commerce_http_enabled_event_t___);
