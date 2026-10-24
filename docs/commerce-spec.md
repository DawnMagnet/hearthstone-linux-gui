# Commerce SDK: observed surface

This document describes what the Hearthstone client does with Blizzard's
commerce SDK (`blz_commerce_sdk_plugin` and `commerce_http_client`), as seen
from the outside. It is the starting point for a clean-room Linux
implementation of the read-only part of the shop (gold balance and catalog),
as discussed in [#26](https://github.com/DawnMagnet/hearthstone-linux-gui/issues/26).

Scope: read-only. Purchases are out of scope and are only listed so that an
implementation knows what to refuse.

## How this was observed

- The Wine bridge from [#46](https://github.com/DawnMagnet/hearthstone-linux-gui/pull/46)
  sits exactly at the boundary the game uses: every SWIG function the game
  calls goes through it. A local call tracer on top of that branch logged
  each call name, its integer and string arguments, its return value, and
  each listener event. Long hex strings (SSO token, OAuth client id, device
  id) were masked while recording.
- The SDK's own log callback (`register_log`) gives the module and parameter
  names and the HTTP endpoints it was configured with.
- Nothing here comes from disassembling the DLLs or reading their code. No
  Blizzard binary is needed to read or use this document.

Capture: one game start to the main menu on the EU region, `fr-FR` locale,
game build 37.0.0.254477, SDK `EcommerceSdk/7.14.1`, 2026-10-24. Account
identifiers are replaced by placeholders below.

## Libraries and calling convention

The game's Mono code calls two native libraries through P/Invoke:

| Library | Exports | Used for |
|---|---|---|
| `blz_commerce_sdk_plugin` | SWIG wrappers named `CSharp_BlizzardfCommerce_<function>___` | the SDK itself |
| `commerce_http_client` | `SetHttpOptions`, `CreateHttpClient`, `DestroyHttpClient` | the HTTP transport handed to the SDK |

In the rest of this document `CSharp_BlizzardfCommerce_` and the trailing
`___` are dropped. Conventions seen at this boundary:

- Structs are opaque pointers. The game creates them with `new_<struct>`,
  fills them with `<struct>_<field>_set`, reads them with
  `<struct>_<field>_get`, and frees them with `delete_<struct>`.
- Strings go in as NUL-terminated UTF-8. Strings coming out are handed to
  the SWIG string callback registered by the game, which copies them into
  managed strings.
- Requests that talk to Blizzard take one JSON string and return a
  `blz_commerce_result_t*` immediately. The answer arrives later as a
  listener event.

## Lifecycle

```
CreateHttpClient / SetHttpOptions
blz_commerce_create → sdk
blz_commerce_http_register_blz_http(sdk, http_client)
blz_commerce_register_log(...)
for module in http, scene, checkout, catalog, vc:
    blz_commerce_register(sdk, blz_commerce_register_<module>())
blz_commerce_add_listener(sdk, 0, listener)
blz_commerce_init(sdk, pairs[4], 4)
blz_catalog_personalized_shop(sdk, json)
blz_commerce_vc_get_balance(sdk, json)   (gold)
blz_commerce_vc_get_balance(sdk, json)   (second currency, XSR)
every frame: blz_commerce_update(sdk) → result; result_t_state_get; delete_result_t
    listener(owner, event) for each finished request
blz_catalog_load_products(sdk, json)     (after the personalized shop answer)
blz_checkout_resume(sdk)
...
on exit: blz_commerce_terminate(sdk), delete_blz_commerce_manifest_t ×5, DestroyHttpClient
```

### 1. HTTP client

`SetHttpOptions(max_connections, timeout_seconds, ca_bundle, disable_server_verify, disable_revoke_check)`
was called with `2, 60, "", 0, 0`. With only two connections, the SDK logs
`HTTP Transport is busy servicing other requests. This request will be
retried.` many times during start-up. That is expected, not an error.

`CreateHttpClient()` returns a handle that the game passes to
`blz_commerce_http_register_blz_http(sdk, handle)`.

### 2. SDK and modules

`blz_commerce_create()` returns a `blz_commerce_sdk_create_result_t*`; the
SDK handle is read with `blz_commerce_sdk_create_result_t_sdk_get`, then the
result is deleted.

Modules are registered in this order, each with
`blz_commerce_register(sdk, manifest)` where the manifest comes from
`blz_commerce_register_<module>()`:

| Order | Function | Module name in the SDK log |
|---|---|---|
| 1 | `register_http` | `http` |
| 2 | `register_scene` | `Scene` |
| 3 | `register_checkout` | `Checkout` |
| 4 | `register_catalog` | `Catalog` |
| 5 | `register_vc` | `vc` |

Then `blz_commerce_add_listener(sdk, 0, listener)`. The listener is a C
function `void listener(owner, event)`; `owner` was always `0`.

### 3. Init parameters

`blz_commerce_init(sdk, pairs, 4)` takes an array (`new_blzCommercePairArray(4)`,
`blzCommercePairArray_setitem`) of `blz_commerce_pair_t` with a string `key`
and a pointer `data`. The keys are read from the SDK's constants
(`BLZ_COMMERCE_HTTP_PARAM_get` and so on).

**`http_param`** → `blz_commerce_http_params_t`

| Field | Observed value | Source in the game |
|---|---|---|
| `title_code` | `WTCG` | constant |
| `title_version` | `37.0.0.254477-MacOS` | game build; the Linux client reports the macOS platform |
| `region` | `2` (EU) | Battle.net region |
| `environment` | `2` (production) | constant |
| `token` | `EU-<32 hex>-<account id>` | single-use Battle.net SSO token (`GenerateSSOToken`) |
| `client_id` | 32 hex characters | sent by the game server |
| `override_gateway_url` | null | |
| `override_oauth_url` | null | |

From these the SDK logs:

- OAuth URL `https://oauth.battle.net/sso`
- API gateway `https://eu.api.blizzard.com`
- OAuth scopes: `commerce.orders.basic:read commerce.shop
  commerce.checkout-app.simple commerce.wallet-app commerce.catalog.basic
  featuredshop.composition.basic commerce.profile.basic
  commerce.catalog.availability:override commerce.virtualcurrency.basic
  commerce.purchase:create`

A read-only client needs at most `commerce.catalog.basic`,
`featuredshop.composition.basic` and `commerce.virtualcurrency.basic`; which
scopes the server actually requires is not verified yet.

**`browser_param`** → `blz_commerce_browser_params_t` (embedded browser used
for checkout; not needed for read-only)

| Field | Observed value |
|---|---|
| `is_prod` | `1` |
| `browser_directory` | `""` (the SDK warns that this field is mandatory) |
| `window_width` / `window_height` | `1296` / `864` |
| `max_window_width` / `max_window_height` | `2880` / `1620` |
| `override_urls` | `1` |
| `navbar_url` | null |
| `checkout_url` | `https://nydus-qa.web.blizzard.net/Bnet/frFR/client/checkout` |
| `log_directory` | the game's `Logs` directory |
| `use_static_asset_loading_page` | `0` |
| `http_environment` | `2` |

**`checkout_browser_param`** → `blz_commerce_checkout_browser_params_t`

| Field | Observed value |
|---|---|
| `title_code` | `WTCG` |
| `title_version` | `37.0.0.254477-MacOS` |
| `device_id` | 32 hex characters |
| `game_service_region` | `"2"` (a string here) |
| `locale` | `fr-FR` |
| `game_account_id` | `""` |

**`checkout_param`** → `blz_commerce_checkout_params_t`:
`supports_legacy_external_platform_purchases = 1`.

## Event delivery

Results reach the game only through polling. Every
frame the game calls `blz_commerce_update(sdk)`, reads
`blz_commerce_result_t_state_get` on the returned result and deletes it.
The state was `0` on almost every frame; `1` was seen six times shortly
after start-up. Its meaning is not known yet.

When a request finishes, the SDK calls the listener from inside
`blz_commerce_update` (all four events in the capture arrived that way). The game then reads the event through getters, and
may call into the SDK again before returning:

```
blz_commerce_event_t_type_get(event)  → module
blz_commerce_event_t_data_get(event)  → module event
    vc:      blz_commerce_vc_event_t_vc_type_get / _vc_data_get
    catalog: blz_commerce_catalog_event_t_catalog_type_get / _catalog_data_get
<specific event>_http_result_get → http result
    blz_commerce_http_enabled_event_t_ok_get → 1 on success
<specific event>_response_get → JSON string
```

| `event_t.type` | Module | Sub-type | Specific event |
|---|---|---|---|
| `2` | vc | `vc_type = 1` | `blz_commerce_vc_get_balance_event_t` |
| `3` | catalog | `catalog_type = 1` | `blz_commerce_catalog_product_load_event_t` |
| `3` | catalog | `catalog_type = 2` | `blz_commerce_catalog_personalized_shop_event_t` |

Only successful answers were observed. The shape of `error` (always `null`
here) and of a failed `http_result` is still unknown.

## Gold balance: `blz_commerce_vc_get_balance`

Request (two calls at start-up):

```json
{"ecosystem":0,"ledgerKey":{"gameServiceRegionId":0,"titleSegment":"T5_HS_EU"},"currencyCode":"XSH"}
{"ecosystem":0,"ledgerKey":{"gameServiceRegionId":0,"titleSegment":""},"currencyCode":"XSR"}
```

- `XSH` is gold. Its `titleSegment` comes from the game server
  (`NetCacheFeatures.Store.VirtualCurrencyTitleSegment`); `T5_HS_EU` was
  seen on the EU region.
- `XSR` uses an empty title segment. Products that grant `XSR` are sold for
  euros at about 100 `XSR` per euro, which matches Runestones; not
  confirmed.

Response (`vc_get_balance_event_t_response_get`):

```json
{
  "error": null,
  "balance": {
    "ledgerId": {
      "accountId": 0,
      "ledgerKey": {"gameServiceRegionId": null, "titleSegment": "T5_HS_EU"},
      "currencyCode": "XSH"
    },
    "purchasedBalance": "0",
    "grantedBalance": "2595",
    "ecosystem": null,
    "ledgerBalanceDetailBreakdown": []
  },
  "softCapped": false
}
```

Amounts are decimal strings. The displayed gold matched
`purchasedBalance + grantedBalance`. For `XSR` the response has
`"ledgerKey": null`. `accountId` is the Battle.net account id (replaced by
`0` here).

## Shop layout: `blz_catalog_personalized_shop`

Request:

```json
{
  "gameServiceRegionId": 2,
  "placementIds": [
    "155c8a1d-b7aa-43ff-9b36-1123c55df906",
    "5c716cd4-65b1-4513-9a42-708953d9346d",
    "0f8ce047-a90a-434c-a3b1-8c2e39258174",
    "477d654a-8c32-4a9d-829a-dc89f4df7423"
  ],
  "storeId": 6,
  "locale": "fr-FR"
}
```

The four placements are the shop tabs. Whether their ids are fixed or sent
by the game server is not checked yet.

Response shape (types instead of values; arrays show one merged element):

```
error: null
metaData: { cacheControl: { maxAgeDurationS: int }, serverTimeMs: int }
placements: [
  placementId: string
  nextScheduledPageDisplayTimeMs: int
  page:
    pageId: string (UUID)
    name: string                 e.g. "37.0 - HEARTHSTONE TAB - Week 0 (10/20)"
    attributes: []
    localization: { name, description, locale }
    sections: [
      sectionId: string
      name: string
      orderInPage: int
      attributes: [ { sectionAttributeKey: string, sectionAttributeValue: string } ]
      localization: { name, description, locale }
      slots: []
      productCollections: [
        productCollectionId: string
        name: string
        orderInSection: int
        personalizationStatus: int
        items: [
          productCollectionItemTypeId: int        (always 1 here)
          productCollectionItemValue: int         (a productId)
          productCollectionTypeId: int
          orderInProductCollection: int
          displayEndTimeMs: int
          personalizedPercentageDiscount: int
          productClaimsToken: string
        ]
      ]
    ]
]
```

Observed: 4 placements (Hearthstone, Featured, Battlegrounds and Modes tabs)
with 41, 13, 9 and 9 sections, 198 items referring to 162 distinct products.
Section attribute keys seen: `layout_map`, `layout_size`, `max_layout_count`,
`Style`/`style`, `override_gold_price_booster`, `subtab_id`, `Tags`,
`tab_icon`, `IsPersonalizedOffer`, `tab_gamemode`, `tab_id`, `tab_order`,
`chain_offer_data`, `tab_enabled`, `platform`, `show_special_fallback_image`,
`subtab_standalone` (and one `layoput_map`, misspelt on the server side).

## Products: `blz_catalog_load_products`

Sent once the personalized shop answer has arrived. Request:

```json
{
  "paginationToken": "",
  "externalPlatformId": 0,
  "productIds": [2151660, 2151659, "... 3529 ids in total"],
  "currencyCodes": [],
  "modifiedSinceFilterMs": 0,
  "gameServiceRegionId": 2,
  "storeId": 6,
  "paginationSize": 300,
  "locale": "fr-FR",
  "requestChildProducts": false
}
```

The id list contains every product referenced by the personalized shop plus
many more; where the rest come from is not established yet (likely the game
server's own product list).

The SDK pages through the results on its own: its log shows five gateway
answers with `paginationToken` `"300"`, `"600"`, `"900"`, `"1200"`, then
`null`. It also fetches the store currency first (`{"currencyAlphaCode":"EUR","error":null}`).
The game gets a single `product_load` event with all products merged.

Response shape:

```
paginationToken: string
metaData: { serverTimeMs: int, cacheControl: { maxAgeDurationS: int } }
childProducts: []
products: [
  productId: int
  name: string
  typeId: int                       (always 0 here)
  serviceItemId: int
  localization: { name, description, locale }
  storeSetting:
    storeId: int
    attributes: [ { key, value } ]
    regionalSetting: { gameServiceRegionId: int, attributes: [ { key, value } ] }
  storeContent: { productImageUri, upsellBundleImageUri, locale }
  prices: [
    currencyCode: string            ("EUR")
    currencyTypeId: int
    originalPrice: string           ("54.99")
    currentPrice: string
    localizedOriginalPrice: string  ("54,99 €")
    localizedCurrentPrice: string
  ]
  virtualCurrencyGrants: [ { currencyCode: string, amount: string } ]
  licenses: [
    licenseId: int
    licenseType: int
    name: string
    licenseAttributes: [ { key, value } ]
  ]
  productBundles: [ { childProductId, weight, bundlePurchaseRuleId, order } ]
  relatedProducts: [ { relationshipTypeId, relatedProductId } ]
  productAvailabilities: [ { startTimeMs, purchaseEndTimeMs, displayEndTimeMs } ]
  ruleIds: [ int ]
  termTypeId: int, termDuration: int
  quantityTermTypeId: int, quantityTermDuration: int
  isServerValidationRequired: bool
  productSubscriptions: []
  productTypeAttributes: []
]
```

Observed: 1375 products returned for 3529 ids requested.

- 581 products have one price, all in `EUR`; 794 have none. No product is
  priced in `XSH`, so **gold prices are not in this answer**. Some may come
  from the `override_gold_price_booster` section attribute; the rest from
  somewhere else still to be found.
- 145 products grant `XSR` through `virtualCurrencyGrants`.
- `licenseAttributes` keys: `product`, `data`, `quantity`, `Minimum Patch`,
  `premium`, `merc_art_variation_id`, `merc_art_variation_premium`, `type`,
  `season`. The most common `product` values: `hiddenlicense`, `booster`,
  `hero`, `mercenaries_currency`, `cardback`, `mercenaries_booster`, `wing`,
  `mercenaries_mercenary`, `randomcard`, `mini_set`, `specific_card`,
  `emote`.

## After loading

`blz_checkout_resume(sdk)` is called once the products are loaded (the SDK
logs `Resuming purchases`), presumably to finish interrupted checkouts. A
read-only implementation can return a successful result and do nothing.

On exit: `blz_commerce_terminate(sdk)`, then `delete_blz_commerce_manifest_t`
for the five manifests, then `DestroyHttpClient()`.

## Out of scope: purchases

These calls exist and must be refused by a read-only implementation, as the
#46 bridge already does: `blz_checkout_purchase`,
`blz_checkout_battlenet_purchase`, `blz_commerce_vc_purchase`. Real-money
checkout stays in Blizzard's own web checkout.

## Open questions

1. **HTTP requests under the SDK.** This capture shows the SWIG surface and
   the base URLs, not the HTTP paths, methods and headers the SDK sends to
   `oauth.battle.net` and `eu.api.blizzard.com`. A native client needs
   them. The next capture should log at the `commerce_http_client`
   boundary.
2. **Token exchange.** How the single-use SSO token becomes a bearer token,
   how long it lasts, and what the game does when it expires.
3. **Gold prices.** Where the gold price of packs and other items comes
   from.
4. **Errors.** Shape of `error`, of a failed `http_result`, and the game's
   behaviour on each.
5. **`result_t` states** and the meaning of `owner` in listener events.
6. **Other regions and locales** (`region`, `titleSegment`, placement ids).
