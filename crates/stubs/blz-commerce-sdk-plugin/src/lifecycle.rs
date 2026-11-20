//! Game-facing lifecycle.
//!
//! `blz_commerce_update` runs calls that were already queued and invokes the
//! listener only after the registry lock is released. A listener that queues
//! `blz_catalog_load_products` does not see that answer until the next update.
//!
//! Purchase entry points on [`hearthstone_linux_gui_commerce::REFUSED_EXPORTS`]
//! return before any transport call. `blz_checkout_resume` returns an ordinary
//! result and does not ask the transport to finish a purchase.
//! Result state stays 0: that is the value on ordinary frames in the capture.
//! State 1 was observed and its meaning is still unknown, so it is never set.

use crate::registry::{Field, Registry, Text};
use crate::strings;
use hearthstone_linux_gui_commerce::{DeliveredEvent, InitInput};
use std::ffi::CStr;
use std::os::raw::c_char;

pub(crate) const HTTP_PARAM: &CStr = c"BLZ_COMMERCE_HTTP_PARAM";
pub(crate) const BROWSER_PARAM: &CStr = c"BLZ_COMMERCE_BROWSER_PARAM";
pub(crate) const CHECKOUT_BROWSER_PARAM: &CStr = c"BLZ_COMMERCE_CHECKOUT_BROWSER_PARAM";
pub(crate) const CHECKOUT_PARAM: &CStr = c"BLZ_COMMERCE_CHECKOUT_PARAM";

/// `event_t.type` for the virtual-currency module. Recorded in the spec.
const MODULE_VC: i64 = 2;
/// `event_t.type` for the catalog module.
const MODULE_CATALOG: i64 = 3;
/// `vc_type` for `vc_get_balance`.
const VC_GET_BALANCE: i64 = 1;
/// `catalog_type` for `blz_catalog_load_products`.
const CATALOG_PRODUCT_LOAD: i64 = 1;
/// `catalog_type` for `blz_catalog_personalized_shop`.
const CATALOG_PERSONALIZED_SHOP: i64 = 2;

pub(crate) fn create() -> isize {
    crate::registry::with_mut(|reg| {
        let sdk = reg.alloc_sdk();
        let result = reg.alloc_result(0);
        reg.set_ptr(result, Field::Sdk, sdk);
        result
    })
}

pub(crate) fn init(sdk: isize, pairs: isize, count: i32) -> isize {
    crate::registry::with_mut(|reg| {
        let input = read_init(reg, pairs, count);
        let exists = if let Some(session) = reg.session_mut(sdk) {
            session.init(input);
            true
        } else {
            false
        };
        if exists {
            reg.alloc_result(0)
        } else {
            0
        }
    })
}

pub(crate) fn update(sdk: isize) -> isize {
    let planned = crate::registry::with_mut(|reg| {
        let (delivered, listener) = {
            let session = reg.session_mut(sdk)?;
            let delivered = session.poll();
            let listener = session.listener();
            (delivered, listener)
        };
        let handles = delivered
            .into_iter()
            .map(|event| materialize(reg, event))
            .collect::<Vec<_>>();
        let result = reg.alloc_result(0);
        Some((listener, handles, result))
    });
    let Some((listener, handles, result)) = planned else {
        return 0;
    };
    if let Some((owner, callback)) = listener {
        if callback != 0 {
            for event in handles {
                strings::invoke_listener(callback, owner as isize, event);
            }
        }
    }
    result
}

pub(crate) fn terminate(sdk: isize) -> isize {
    crate::registry::with_mut(|reg| {
        let exists = if let Some(session) = reg.session_mut(sdk) {
            session.terminate();
            true
        } else {
            false
        };
        if exists {
            reg.alloc_result(0)
        } else {
            0
        }
    })
}

pub(crate) fn register_module(sdk: isize, manifest: isize) -> isize {
    crate::registry::with_mut(|reg| {
        let name = reg.label(manifest);
        if !name.is_empty() {
            if let Some(session) = reg.session_mut(sdk) {
                session.register_module(name);
            }
        }
        0
    })
}

pub(crate) fn manifest(name: &'static str) -> isize {
    crate::registry::with_mut(|reg| reg.alloc_labeled(name))
}

pub(crate) fn add_listener(sdk: isize, owner: isize, listener: usize) -> isize {
    crate::registry::with_mut(|reg| {
        if let Some(session) = reg.session_mut(sdk) {
            session.set_listener(owner as i64, listener);
        }
        0
    })
}

pub(crate) fn register_http_client(sdk: isize, client: isize) -> isize {
    crate::registry::with_mut(|reg| {
        if let Some(session) = reg.session_mut(sdk) {
            session.set_http_client(client as i64);
        }
        0
    })
}

pub(crate) fn balance(sdk: isize, request: *const c_char) -> isize {
    enqueue(sdk, strings::lossy(request), |session, json| {
        session.enqueue_balance(json);
    })
}

pub(crate) fn shop(sdk: isize, request: *const c_char) -> isize {
    enqueue(sdk, strings::lossy(request), |session, json| {
        session.enqueue_shop(json);
    })
}

pub(crate) fn products(sdk: isize, request: *const c_char) -> isize {
    enqueue(sdk, strings::lossy(request), |session, json| {
        session.enqueue_products(json);
    })
}

/// Refuses one symbol from [`hearthstone_linux_gui_commerce::REFUSED_EXPORTS`].
///
/// Returns a null result before any transport call and does not build a
/// failure document. A symbol that is not on the list also returns null; it
/// is not counted as a purchase refusal.
pub(crate) fn refuse_listed(sdk: isize, export: &str) -> isize {
    if !hearthstone_linux_gui_commerce::is_refused_export(export) {
        return 0;
    }
    crate::registry::with_mut(|reg| {
        if let Some(session) = reg.session_mut(sdk) {
            session.refuse_purchase();
        }
        0
    })
}

pub(crate) fn resume(sdk: isize) -> isize {
    crate::registry::with_mut(|reg| {
        let exists = if let Some(session) = reg.session_mut(sdk) {
            session.resume();
            true
        } else {
            false
        };
        if exists {
            reg.alloc_result(0)
        } else {
            0
        }
    })
}

fn enqueue(
    sdk: isize,
    json: String,
    queue: impl FnOnce(&mut hearthstone_linux_gui_commerce::SdkSession, &str),
) -> isize {
    crate::registry::with_mut(|reg| {
        let exists = if let Some(session) = reg.session_mut(sdk) {
            queue(session, &json);
            true
        } else {
            false
        };
        if exists {
            reg.alloc_result(0)
        } else {
            0
        }
    })
}

fn materialize(reg: &mut Registry, event: DeliveredEvent) -> isize {
    let (module, sub, json) = match event {
        DeliveredEvent::Balance(json) => (MODULE_VC, VC_GET_BALANCE, json),
        DeliveredEvent::ProductLoad(json) => (MODULE_CATALOG, CATALOG_PRODUCT_LOAD, json),
        DeliveredEvent::PersonalizedShop(json) => (MODULE_CATALOG, CATALOG_PERSONALIZED_SHOP, json),
    };
    // `ok` was 1 on every success in the capture. Numeric result and system
    // codes were not recorded, so they stay 0 rather than a guessed status.
    let http = reg.alloc_plain();
    reg.set_flag(http, Field::HttpOk, true);
    reg.set_int(http, Field::ResultCode, 0);
    reg.set_int(http, Field::SystemCode, 0);
    reg.set_text(http, Field::HttpMessage, Text::Value(String::new()));

    let specific = reg.alloc_plain();
    reg.set_text(specific, Field::Response, Text::Value(json));
    reg.set_ptr(specific, Field::HttpResult, http);

    let middle = reg.alloc_plain();
    if module == MODULE_VC {
        reg.set_int(middle, Field::VcType, sub);
        reg.set_ptr(middle, Field::VcData, specific);
    } else {
        reg.set_int(middle, Field::CatalogType, sub);
        reg.set_ptr(middle, Field::CatalogData, specific);
    }

    let outer = reg.alloc_plain();
    reg.set_int(outer, Field::Type, module);
    reg.set_ptr(outer, Field::Data, middle);
    reg.set_int(outer, Field::ReferenceId, 0);
    outer
}

fn read_init(reg: &Registry, pairs: isize, count: i32) -> InitInput {
    let mut input = InitInput::default();
    let count = usize::try_from(count).unwrap_or(0);
    for handle in reg.items(pairs).into_iter().take(count) {
        if handle == 0 {
            continue;
        }
        let key = match reg.text(handle, Field::PairKey) {
            Text::Value(value) => value,
            _ => continue,
        };
        let data = reg.ptr(handle, Field::Data);
        if data == 0 {
            continue;
        }
        if key_is(&key, HTTP_PARAM) {
            input.title_code = owned(reg, data, Field::TitleCode);
            input.title_version = owned(reg, data, Field::TitleVersion);
            input.region_id = i32::try_from(reg.int(data, Field::Region)).unwrap_or(0);
            input.environment = i32::try_from(reg.int(data, Field::Environment)).unwrap_or(0);
            input.token = owned(reg, data, Field::Token);
            input.client_id = owned(reg, data, Field::ClientId);
            input.gateway_override = optional(reg, data, Field::OverrideGateway);
            input.oauth_override = optional(reg, data, Field::OverrideOauth);
        } else if key_is(&key, CHECKOUT_BROWSER_PARAM) {
            input.locale = owned(reg, data, Field::Locale);
            input.game_service_region = owned(reg, data, Field::GameServiceRegion);
            input.device_id = owned(reg, data, Field::DeviceId);
        } else if key_is(&key, CHECKOUT_PARAM) {
            input.supports_legacy_external_purchases = reg.flag(data, Field::SupportsLegacy);
        }
    }
    input
}

fn key_is(value: &str, key: &CStr) -> bool {
    key.to_str().ok() == Some(value)
}

fn owned(reg: &Registry, handle: isize, field: Field) -> String {
    match reg.text(handle, field) {
        Text::Value(value) => value,
        _ => String::new(),
    }
}

fn optional(reg: &Registry, handle: isize, field: Field) -> Option<String> {
    match reg.text(handle, field) {
        Text::Value(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}
