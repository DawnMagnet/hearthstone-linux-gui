# Read-only commerce implementation

This note describes the Linux stand-in for the surface in
[commerce-spec.md](commerce-spec.md). It is not a Blizzard SDK. It does not
contain Blizzard code, it does not load Wine, and it does not ship official
DLLs. Purchase calls are refused in-process. Nothing in the default build
opens a socket.

The observation document stays the source of what was seen. This note only
says what the code does with that surface, and what it refuses to invent.

## What is implemented

`libblz_commerce_sdk_plugin.so` and `libcommerce_http_client.so` keep the
names the game P/Invokes.

- Opaque handles (`new_*`, field get/set, `delete_*`) and the SWIG string
  callback `SWIGRegisterStringCallback_battlenet_commerce`.
- The startup order from the spec: HTTP client options, SDK create, module
  registration (`http`, `Scene`, `Checkout`, `Catalog`, `vc`), listener, and
  `blz_commerce_init` over the four parameter pairs. Region, locale, title,
  title segment, and placement ids are copied from those pairs and from the
  request JSON. They are not constants.
- Read-only calls return a result immediately and queue the work.
  `blz_commerce_update` runs the calls that were already queued and invokes
  the listener after it drops its lock, so a listener can enqueue
  `blz_catalog_load_products` and that answer waits for the next frame.
- Delivered events use the recorded type numbers: virtual currency `2` with
  balance subtype `1`, catalog `3` with product load `1` and personalized
  shop `2`. On a delivered success, `http_result.ok` is true. Numeric
  `result_code` and `system_code` were not in the capture, so they stay `0`.
  Update and resume results use state `0`, the value seen on ordinary frames.
  State `1` was observed and is not used, because its meaning is unknown.
- JSON for `vc_get_balance` (`XSH` and `XSR`), `blz_catalog_personalized_shop`,
  and `blz_catalog_load_products`. Amounts stay decimal strings. Unknown keys
  are kept. A non-null `error` is not turned into a success event, because
  the error document was not observed.
- The displayed-gold sum (`purchasedBalance + grantedBalance`) is a pure
  helper. It is not written back into the response.
- `CommerceTransport`. `FixtureReplay` feeds tests. `NotYetSpecifiedTransport`
  is what the game process uses. Neither implementation sends bytes.
- Purchase exports are `REFUSED_EXPORTS` in `crates/commerce` (the dynamic
  symbols for `blz_checkout_purchase`, `blz_checkout_battlenet_purchase`, and
  `blz_commerce_vc_purchase`). That list is the only source. Each one returns
  a null result before the transport is touched. No failure JSON is invented.
  A commerce-bridge must refuse those symbols before writing the call into
  the pipe. `blz_checkout_resume` returns an ordinary result and does nothing.
- Hosts. The only recorded row is the EU capture: region id `2`, gateway
  `https://eu.api.blizzard.com`, OAuth `https://oauth.battle.net/sso`. A
  non-empty `override_gateway_url` or `override_oauth_url` from the game
  replaces that side. Every other region id stays empty.

`commerce_http_client` stores `SetHttpOptions` (the capture called it with
`2, 60, "", 0, 0`) and returns a client id from `CreateHttpClient`. The SDK
records that id. The two libraries do not share a connection pool, because
the HTTP exchange itself is not specified. The "transport busy, retry" log
from the real SDK is not simulated.

## What is still blocked

These are the open questions in the spec. The code does not answer them:

- Paths, methods, and headers for the OAuth host and the regional gateway.
- How the single-use SSO token becomes a bearer token, and what the game does
  when it expires. Tokens are kept only in the session and are redacted from
  `Debug`.
- Where gold prices come from. The captured product list was not priced in
  `XSH`. This client does not add a gold price.
- The shape of `error`, of a failed `http_result`, and of `result_t` state `1`.
- Gateway paging. The real SDK merged product pages and then emitted one
  SWIG event. Page requests are unspecified, so this client makes one
  transport call per SWIG call and does not synthesize the next page. A later
  transport can do several reads inside one `execute` and return the merged
  document, once those reads have actually been observed.
- CN and EU. The capture was EU / `fr-FR`. The owner's Windows client is
  region CN, locale `zhCN`, Battle.net product `37.0.0.0` / data `37003`.
  No CN host, region number, title segment, or placement id is hardcoded.
  Those have to be confirmed on a CN session, separately from the EU row.

Until that HTTP exchange is observed, the libraries the game loads will not
open the shop against a live service. Tests replay fixtures and check that
the default path does not call out.

## Where the code lives

| Crate | Role |
| --- | --- |
| `crates/commerce` | Session, JSON shapes, region table, transport trait |
| `crates/stubs/blz-commerce-sdk-plugin` | `libblz_commerce_sdk_plugin.so` |
| `crates/stubs/commerce-http-client` | `libcommerce_http_client.so` |

```sh
cargo test -p hearthstone-linux-gui-commerce \
  -p hearthstone-linux-gui-blz-commerce-sdk-plugin-stub \
  -p hearthstone-linux-gui-commerce-http-client-stub
```

Those tests do not use the network and do not contact a Blizzard service.
