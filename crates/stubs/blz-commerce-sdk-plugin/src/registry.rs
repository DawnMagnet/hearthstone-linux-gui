//! Opaque handles for the SWIG structs.
//!
//! The game only sees integers. Zero is null. Each live object is a bag of
//! fields plus, for the SDK handle, the read-only session.

use crate::strings;
#[cfg(test)]
use hearthstone_linux_gui_commerce::CommerceTransport;
use hearthstone_linux_gui_commerce::SdkSession;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Field {
    ClientId,
    Token,
    TitleCode,
    TitleVersion,
    OverrideGateway,
    OverrideOauth,
    LogDirectory,
    BrowserDirectory,
    CheckoutUrl,
    NavbarUrl,
    DeviceId,
    Locale,
    GameServiceRegion,
    GameAccountId,
    RoutingKey,
    ServerValidationSignature,
    SsoToken,
    ExternalTransactionId,
    PurchaseProductId,
    PurchaseCurrencyId,
    ProductClaimsToken,
    CancelTransactionId,
    PairKey,
    Response,
    HttpMessage,
    Region,
    Environment,
    WindowWidth,
    WindowHeight,
    MaxWindowWidth,
    MaxWindowHeight,
    HttpEnvironment,
    State,
    ReferenceId,
    Type,
    CatalogType,
    VcType,
    VecX,
    VecY,
    DependencyCount,
    ResultCode,
    SystemCode,
    IsProd,
    UseStaticAssetLoadingPage,
    SupportsLegacy,
    HttpOk,
    Sdk,
    Data,
    BrowserPurchase,
    ManifestConfig,
    ManifestPostInit,
    ManifestTerminate,
    ManifestUpdate,
    ManifestGetName,
    ManifestGetScopes,
    ManifestDependencies,
    HttpResult,
    CatalogData,
    VcData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Text {
    Unset,
    Null,
    Value(String),
}

enum Slot {
    Text(Text),
    Int(i64),
    Bool(bool),
    Ptr(isize),
}

struct Object {
    fields: HashMap<Field, Slot>,
    items: Vec<isize>,
    label: &'static str,
    session: Option<SdkSession>,
}

pub(crate) struct Registry {
    next: u64,
    objects: HashMap<u64, Object>,
    log_owner: isize,
    log_hook: isize,
}

fn lock() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            next: 0,
            objects: HashMap::new(),
            log_owner: 0,
            log_hook: 0,
        })
    })
}

pub(crate) fn with_mut<R>(body: impl FnOnce(&mut Registry) -> R) -> R {
    let mut guard = lock().lock().unwrap_or_else(|error| error.into_inner());
    body(&mut guard)
}

pub(crate) fn set_text(handle: isize, field: Field, value: *const std::os::raw::c_char) {
    let text = strings::text_from_ptr(value);
    with_mut(|reg| reg.set_text(handle, field, text));
}

pub(crate) fn text(handle: isize, field: Field) -> Text {
    with_mut(|reg| reg.text(handle, field))
}

pub(crate) fn set_int(handle: isize, field: Field, value: i64) {
    with_mut(|reg| reg.set_int(handle, field, value));
}

pub(crate) fn int(handle: isize, field: Field) -> i64 {
    with_mut(|reg| reg.int(handle, field))
}

pub(crate) fn set_flag(handle: isize, field: Field, value: bool) {
    with_mut(|reg| reg.set_flag(handle, field, value));
}

pub(crate) fn flag(handle: isize, field: Field) -> bool {
    with_mut(|reg| reg.flag(handle, field))
}

pub(crate) fn set_ptr(handle: isize, field: Field, value: isize) {
    with_mut(|reg| reg.set_ptr(handle, field, value));
}

pub(crate) fn ptr(handle: isize, field: Field) -> isize {
    with_mut(|reg| reg.ptr(handle, field))
}

pub(crate) fn alloc_plain() -> isize {
    with_mut(|reg| reg.alloc_plain())
}

pub(crate) fn alloc_list(size: i32) -> isize {
    with_mut(|reg| reg.alloc_list(size))
}

pub(crate) fn free(handle: isize) {
    with_mut(|reg| reg.free(handle));
}

pub(crate) fn list_set(handle: isize, index: i32, value: isize) {
    with_mut(|reg| reg.list_set(handle, index, value));
}

pub(crate) fn set_log(owner: isize, hook: isize) {
    with_mut(|reg| {
        reg.log_owner = owner;
        reg.log_hook = hook;
    });
}

pub(crate) fn clear_log(owner: isize) {
    with_mut(|reg| {
        if reg.log_owner == owner {
            reg.log_owner = 0;
            reg.log_hook = 0;
        }
    });
}

impl Registry {
    pub(crate) fn alloc_plain(&mut self) -> isize {
        self.insert(Object::plain())
    }

    pub(crate) fn alloc_sdk(&mut self) -> isize {
        let mut object = Object::plain();
        object.session = Some(SdkSession::default());
        self.insert(object)
    }

    pub(crate) fn alloc_labeled(&mut self, label: &'static str) -> isize {
        let mut object = Object::plain();
        object.label = label;
        self.insert(object)
    }

    pub(crate) fn alloc_list(&mut self, size: i32) -> isize {
        let len = usize::try_from(size).unwrap_or(0).min(1024);
        let mut object = Object::plain();
        object.items = vec![0; len];
        self.insert(object)
    }

    pub(crate) fn alloc_result(&mut self, state: i64) -> isize {
        let handle = self.alloc_plain();
        self.set_int(handle, Field::State, state);
        handle
    }

    pub(crate) fn free(&mut self, handle: isize) {
        if let Some(id) = id_of(handle) {
            self.objects.remove(&id);
        }
    }

    pub(crate) fn set_text(&mut self, handle: isize, field: Field, text: Text) {
        if let Some(object) = self.object_mut(handle) {
            object.fields.insert(field, Slot::Text(text));
        }
    }

    pub(crate) fn text(&self, handle: isize, field: Field) -> Text {
        match self.slot(handle, field) {
            Some(Slot::Text(text)) => text.clone(),
            _ => Text::Unset,
        }
    }

    pub(crate) fn set_int(&mut self, handle: isize, field: Field, value: i64) {
        if let Some(object) = self.object_mut(handle) {
            object.fields.insert(field, Slot::Int(value));
        }
    }

    pub(crate) fn int(&self, handle: isize, field: Field) -> i64 {
        match self.slot(handle, field) {
            Some(Slot::Int(value)) => *value,
            _ => 0,
        }
    }

    pub(crate) fn set_flag(&mut self, handle: isize, field: Field, value: bool) {
        if let Some(object) = self.object_mut(handle) {
            object.fields.insert(field, Slot::Bool(value));
        }
    }

    pub(crate) fn flag(&self, handle: isize, field: Field) -> bool {
        match self.slot(handle, field) {
            Some(Slot::Bool(value)) => *value,
            _ => false,
        }
    }

    pub(crate) fn set_ptr(&mut self, handle: isize, field: Field, value: isize) {
        if let Some(object) = self.object_mut(handle) {
            object.fields.insert(field, Slot::Ptr(value));
        }
    }

    pub(crate) fn ptr(&self, handle: isize, field: Field) -> isize {
        match self.slot(handle, field) {
            Some(Slot::Ptr(value)) => *value,
            _ => 0,
        }
    }

    pub(crate) fn list_set(&mut self, handle: isize, index: i32, value: isize) {
        let Some(index) = usize::try_from(index).ok() else {
            return;
        };
        let Some(object) = self.object_mut(handle) else {
            return;
        };
        if let Some(slot) = object.items.get_mut(index) {
            *slot = value;
        }
    }

    pub(crate) fn items(&self, handle: isize) -> Vec<isize> {
        self.object(handle)
            .map(|object| object.items.clone())
            .unwrap_or_default()
    }

    pub(crate) fn label(&self, handle: isize) -> &'static str {
        self.object(handle).map(|object| object.label).unwrap_or("")
    }

    pub(crate) fn session_mut(&mut self, handle: isize) -> Option<&mut SdkSession> {
        self.object_mut(handle)
            .and_then(|object| object.session.as_mut())
    }

    #[cfg(test)]
    pub(crate) fn session(&self, handle: isize) -> Option<&SdkSession> {
        self.object(handle)
            .and_then(|object| object.session.as_ref())
    }

    #[cfg(test)]
    pub(crate) fn set_transport(&mut self, handle: isize, transport: Box<dyn CommerceTransport>) {
        if let Some(session) = self.session_mut(handle) {
            session.set_transport(transport);
        }
    }

    fn insert(&mut self, object: Object) -> isize {
        let Some(id) = self.next.checked_add(1) else {
            return 0;
        };
        self.next = id;
        self.objects.insert(id, object);
        isize::try_from(id).unwrap_or(0)
    }

    fn object(&self, handle: isize) -> Option<&Object> {
        id_of(handle).and_then(|id| self.objects.get(&id))
    }

    fn object_mut(&mut self, handle: isize) -> Option<&mut Object> {
        id_of(handle).and_then(|id| self.objects.get_mut(&id))
    }

    fn slot(&self, handle: isize, field: Field) -> Option<&Slot> {
        self.object(handle)
            .and_then(|object| object.fields.get(&field))
    }
}

impl Object {
    fn plain() -> Self {
        Self {
            fields: HashMap::new(),
            items: Vec::new(),
            label: "",
            session: None,
        }
    }
}

fn id_of(handle: isize) -> Option<u64> {
    u64::try_from(handle).ok().filter(|id| *id != 0)
}

#[cfg(test)]
pub(crate) fn reset() {
    with_mut(|reg| {
        reg.next = 0;
        reg.objects.clear();
        reg.log_owner = 0;
        reg.log_hook = 0;
    });
}

#[cfg(test)]
pub(crate) fn testing_view(sdk: isize) -> Option<hearthstone_linux_gui_commerce::SessionView> {
    with_mut(|reg| reg.session(sdk).map(|session| session.view()))
}

#[cfg(test)]
pub(crate) fn testing_requests(sdk: isize) -> Vec<hearthstone_linux_gui_commerce::ReadOnlyCall> {
    with_mut(|reg| {
        reg.session(sdk)
            .map(|session| session.requests().to_vec())
            .unwrap_or_default()
    })
}

#[cfg(test)]
pub(crate) fn testing_set_transport(sdk: isize, transport: Box<dyn CommerceTransport>) {
    with_mut(|reg| reg.set_transport(sdk, transport));
}

#[cfg(test)]
pub(crate) fn testing_log() -> (isize, isize) {
    with_mut(|reg| (reg.log_owner, reg.log_hook))
}
