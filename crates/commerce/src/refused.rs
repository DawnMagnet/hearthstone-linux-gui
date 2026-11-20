//! Purchase exports a read-only client refuses.
//!
//! [`REFUSED_EXPORTS`] is the only list. The native stub and the #46
//! commerce-bridge both take the names from here. The bridge must refuse a
//! matching symbol before that call is written into the pipe.

/// Dynamic symbol for `blz_checkout_purchase`.
pub const EXPORT_CHECKOUT_PURCHASE: &str = REFUSED_EXPORTS[0];

/// Dynamic symbol for `blz_checkout_battlenet_purchase`.
pub const EXPORT_CHECKOUT_BATTLENET_PURCHASE: &str = REFUSED_EXPORTS[1];

/// Dynamic symbol for `blz_commerce_vc_purchase`.
pub const EXPORT_COMMERCE_VC_PURCHASE: &str = REFUSED_EXPORTS[2];

/// The only list of purchase exports a read-only client refuses.
///
/// Each entry is the dynamic symbol of `libblz_commerce_sdk_plugin` that the
/// game P/Invokes (`CSharp_BlizzardfCommerce_`, the `blz_` function, and the
/// trailing `___`). The native stub and the #46 commerce-bridge must both use
/// this list. The bridge must refuse a matching symbol before the call is
/// written into the pipe.
///
/// A refusal returns a null result before the transport runs and does not
/// invent a failure JSON document. `blz_checkout_cancel_purchase` and
/// `blz_checkout_resume` are not purchases and are not listed.
pub const REFUSED_EXPORTS: &[&str] = &[
    "CSharp_BlizzardfCommerce_blz_checkout_purchase___",
    "CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___",
    "CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___",
];

/// Whether `symbol` is a purchase export that must be refused in-process.
pub fn is_refused_export(symbol: &str) -> bool {
    REFUSED_EXPORTS.contains(&symbol)
}

#[cfg(test)]
mod tests {
    use super::{
        is_refused_export, EXPORT_CHECKOUT_BATTLENET_PURCHASE, EXPORT_CHECKOUT_PURCHASE,
        EXPORT_COMMERCE_VC_PURCHASE, REFUSED_EXPORTS,
    };

    #[test]
    fn refused_exports_are_the_three_purchase_symbols() {
        assert_eq!(
            REFUSED_EXPORTS,
            [
                "CSharp_BlizzardfCommerce_blz_checkout_purchase___",
                "CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___",
                "CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___",
            ]
        );
        assert_eq!(EXPORT_CHECKOUT_PURCHASE, REFUSED_EXPORTS[0]);
        assert_eq!(EXPORT_CHECKOUT_BATTLENET_PURCHASE, REFUSED_EXPORTS[1]);
        assert_eq!(EXPORT_COMMERCE_VC_PURCHASE, REFUSED_EXPORTS[2]);
        assert!(EXPORT_CHECKOUT_PURCHASE.ends_with("blz_checkout_purchase___"));
        assert!(EXPORT_CHECKOUT_BATTLENET_PURCHASE.ends_with("blz_checkout_battlenet_purchase___"));
        assert!(EXPORT_COMMERCE_VC_PURCHASE.ends_with("blz_commerce_vc_purchase___"));

        let mut unique = REFUSED_EXPORTS.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), REFUSED_EXPORTS.len());
    }

    #[test]
    fn only_the_full_purchase_symbols_are_refused() {
        for symbol in REFUSED_EXPORTS {
            assert!(is_refused_export(symbol), "{symbol}");
        }
        for symbol in [
            "checkout_purchase",
            "checkout_battlenet_purchase",
            "commerce_vc_purchase",
            "blz_checkout_purchase",
            "blz_checkout_battlenet_purchase",
            "blz_commerce_vc_purchase",
            "CSharp_BlizzardfCommerce_blz_checkout_cancel_purchase___",
            "CSharp_BlizzardfCommerce_blz_checkout_resume___",
            "CSharp_BlizzardfCommerce_blz_commerce_vc_get_balance___",
            "",
        ] {
            assert!(!is_refused_export(symbol), "{symbol}");
        }
    }
}
