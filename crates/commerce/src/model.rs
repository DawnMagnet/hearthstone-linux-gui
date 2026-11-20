//! JSON documents at the SWIG boundary.
//!
//! Field names and types follow `docs/commerce-spec.md`. A key whose JSON
//! type the capture did not show is a [`serde_json::Value`]. Unknown keys are
//! kept when a fixture is accepted, so a later field does not have to be
//! invented here before it can pass through. `error` is accepted only as
//! JSON null: the failure document was not observed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRequest {
    pub ecosystem: i64,
    pub ledger_key: RequestLedgerKey,
    pub currency_code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestLedgerKey {
    pub game_service_region_id: i64,
    pub title_segment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceResponse {
    pub error: Option<Value>,
    pub balance: BalanceBody,
    pub soft_capped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceBody {
    pub ledger_id: LedgerId,
    pub purchased_balance: String,
    pub granted_balance: String,
    pub ecosystem: Option<Value>,
    pub ledger_balance_detail_breakdown: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerId {
    pub account_id: i64,
    pub ledger_key: Option<ResponseLedgerKey>,
    pub currency_code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseLedgerKey {
    pub game_service_region_id: Option<i64>,
    pub title_segment: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizedShopRequest {
    pub game_service_region_id: i64,
    pub placement_ids: Vec<String>,
    pub store_id: i64,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalizedShopResponse {
    pub error: Option<Value>,
    pub meta_data: MetaData,
    pub placements: Vec<Placement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaData {
    pub cache_control: CacheControl,
    pub server_time_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheControl {
    pub max_age_duration_s: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub placement_id: String,
    pub next_scheduled_page_display_time_ms: i64,
    pub page: ShopPage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopPage {
    pub page_id: String,
    pub name: String,
    pub attributes: Vec<Value>,
    pub localization: Localization,
    pub sections: Vec<ShopSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Localization {
    pub name: String,
    pub description: String,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopSection {
    pub section_id: String,
    pub name: String,
    pub order_in_page: i64,
    pub attributes: Vec<SectionAttribute>,
    pub localization: Localization,
    pub slots: Vec<Value>,
    pub product_collections: Vec<ProductCollection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionAttribute {
    pub section_attribute_key: String,
    pub section_attribute_value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductCollection {
    pub product_collection_id: String,
    pub name: String,
    pub order_in_section: i64,
    pub personalization_status: i64,
    pub items: Vec<CollectionItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionItem {
    pub product_collection_item_type_id: i64,
    pub product_collection_item_value: i64,
    pub product_collection_type_id: i64,
    pub order_in_product_collection: i64,
    pub display_end_time_ms: i64,
    pub personalized_percentage_discount: i64,
    pub product_claims_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadProductsRequest {
    pub pagination_token: String,
    pub external_platform_id: i64,
    pub product_ids: Vec<i64>,
    pub currency_codes: Vec<String>,
    pub modified_since_filter_ms: i64,
    pub game_service_region_id: i64,
    pub store_id: i64,
    pub pagination_size: i64,
    pub locale: String,
    pub request_child_products: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadProductsResponse {
    pub pagination_token: Option<String>,
    pub meta_data: MetaData,
    pub child_products: Vec<Value>,
    pub products: Vec<CatalogProduct>,
    /// Not part of the recorded product document. Null or absent is fine.
    /// A non-null value is an unspecified error document.
    #[serde(default)]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProduct {
    pub product_id: i64,
    pub name: String,
    pub type_id: i64,
    pub service_item_id: i64,
    pub localization: Localization,
    pub store_setting: StoreSetting,
    pub store_content: StoreContent,
    pub prices: Vec<Price>,
    pub virtual_currency_grants: Vec<VirtualCurrencyGrant>,
    pub licenses: Vec<License>,
    pub product_bundles: Vec<ProductBundle>,
    pub related_products: Vec<RelatedProduct>,
    pub product_availabilities: Vec<Availability>,
    pub rule_ids: Vec<i64>,
    pub term_type_id: i64,
    pub term_duration: i64,
    pub quantity_term_type_id: i64,
    pub quantity_term_duration: i64,
    pub is_server_validation_required: bool,
    pub product_subscriptions: Vec<Value>,
    pub product_type_attributes: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreSetting {
    pub store_id: i64,
    pub attributes: Vec<KeyValue>,
    pub regional_setting: RegionalSetting,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionalSetting {
    pub game_service_region_id: i64,
    pub attributes: Vec<KeyValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreContent {
    pub product_image_uri: String,
    pub upsell_bundle_image_uri: String,
    pub locale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Price {
    pub currency_code: String,
    pub currency_type_id: i64,
    pub original_price: String,
    pub current_price: String,
    pub localized_original_price: String,
    pub localized_current_price: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualCurrencyGrant {
    pub currency_code: String,
    pub amount: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct License {
    pub license_id: i64,
    pub license_type: i64,
    pub name: String,
    pub license_attributes: Vec<KeyValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductBundle {
    pub child_product_id: i64,
    /// The capture names `weight` and does not show a sample value.
    pub weight: Value,
    pub bundle_purchase_rule_id: i64,
    pub order: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedProduct {
    pub relationship_type_id: i64,
    pub related_product_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub start_time_ms: i64,
    pub purchase_end_time_ms: i64,
    pub display_end_time_ms: i64,
}

pub(crate) fn error_is_unspecified(error: &Option<Value>) -> bool {
    error.is_some()
}

/// Synthetic documents in the recorded shape. They are not account data and
/// they are not an HTTP capture.
pub mod fixtures {
    use super::{
        Availability, BalanceBody, BalanceResponse, CacheControl, CatalogProduct, CollectionItem,
        KeyValue, LedgerId, License, LoadProductsResponse, Localization, MetaData, Placement,
        Price, ProductCollection, RegionalSetting, ResponseLedgerKey, ShopPage, ShopSection,
        StoreContent, StoreSetting, VirtualCurrencyGrant,
    };
    use serde_json::{json, Value};

    pub fn balance(
        currency: &str,
        title_segment: Option<&str>,
        account_id: i64,
        purchased: &str,
        granted: &str,
    ) -> Value {
        let ledger_key = title_segment.map(|segment| ResponseLedgerKey {
            game_service_region_id: None,
            title_segment: segment.to_string(),
        });
        let document = BalanceResponse {
            error: None,
            soft_capped: false,
            balance: BalanceBody {
                ledger_id: LedgerId {
                    account_id,
                    ledger_key,
                    currency_code: currency.to_string(),
                },
                purchased_balance: purchased.to_string(),
                granted_balance: granted.to_string(),
                ecosystem: None,
                ledger_balance_detail_breakdown: Vec::new(),
            },
        };
        serde_json::to_value(document).expect("balance fixture")
    }

    pub fn personalized_shop(
        placement_id: &str,
        locale: &str,
        product_id: i64,
        section_attribute_key: &str,
        section_attribute_value: &str,
    ) -> Value {
        let localization = Localization {
            name: "tab".to_string(),
            description: String::new(),
            locale: locale.to_string(),
        };
        let document = super::PersonalizedShopResponse {
            error: None,
            meta_data: meta(),
            placements: vec![Placement {
                placement_id: placement_id.to_string(),
                next_scheduled_page_display_time_ms: 0,
                page: ShopPage {
                    page_id: "page-from-fixture".to_string(),
                    name: "fixture page".to_string(),
                    attributes: Vec::new(),
                    localization: localization.clone(),
                    sections: vec![ShopSection {
                        section_id: "section-from-fixture".to_string(),
                        name: "fixture section".to_string(),
                        order_in_page: 0,
                        attributes: vec![super::SectionAttribute {
                            section_attribute_key: section_attribute_key.to_string(),
                            section_attribute_value: section_attribute_value.to_string(),
                        }],
                        localization,
                        slots: Vec::new(),
                        product_collections: vec![ProductCollection {
                            product_collection_id: "collection-from-fixture".to_string(),
                            name: "fixture collection".to_string(),
                            order_in_section: 0,
                            personalization_status: 0,
                            items: vec![CollectionItem {
                                product_collection_item_type_id: 1,
                                product_collection_item_value: product_id,
                                product_collection_type_id: 0,
                                order_in_product_collection: 0,
                                display_end_time_ms: 0,
                                personalized_percentage_discount: 0,
                                product_claims_token: String::new(),
                            }],
                        }],
                    }],
                },
            }],
        };
        serde_json::to_value(document).expect("shop fixture")
    }

    pub fn load_products(_locale: &str, products: Vec<CatalogProduct>) -> Value {
        let document = LoadProductsResponse {
            pagination_token: None,
            meta_data: meta(),
            child_products: Vec::new(),
            products,
            error: None,
        };
        serde_json::to_value(document).expect("product fixture")
    }

    pub fn priced_product(
        product_id: i64,
        locale: &str,
        currency: &str,
        price: &str,
    ) -> CatalogProduct {
        let mut product = bare_product(product_id, locale);
        product.prices.push(Price {
            currency_code: currency.to_string(),
            currency_type_id: 0,
            original_price: price.to_string(),
            current_price: price.to_string(),
            localized_original_price: price.to_string(),
            localized_current_price: price.to_string(),
        });
        product
    }

    pub fn grant_product(
        product_id: i64,
        locale: &str,
        currency: &str,
        amount: &str,
    ) -> CatalogProduct {
        let mut product = bare_product(product_id, locale);
        product.virtual_currency_grants.push(VirtualCurrencyGrant {
            currency_code: currency.to_string(),
            amount: amount.to_string(),
        });
        product
    }

    fn bare_product(product_id: i64, locale: &str) -> CatalogProduct {
        CatalogProduct {
            product_id,
            name: format!("product {product_id}"),
            type_id: 0,
            service_item_id: 0,
            localization: Localization {
                name: format!("product {product_id}"),
                description: String::new(),
                locale: locale.to_string(),
            },
            store_setting: StoreSetting {
                store_id: 6,
                attributes: vec![KeyValue {
                    key: "fixture".to_string(),
                    value: "1".to_string(),
                }],
                regional_setting: RegionalSetting {
                    game_service_region_id: 0,
                    attributes: Vec::new(),
                },
            },
            store_content: StoreContent {
                product_image_uri: String::new(),
                upsell_bundle_image_uri: String::new(),
                locale: locale.to_string(),
            },
            prices: Vec::new(),
            virtual_currency_grants: Vec::new(),
            licenses: vec![License {
                license_id: 1,
                license_type: 0,
                name: "fixture-license".to_string(),
                license_attributes: vec![KeyValue {
                    key: "product".to_string(),
                    value: "booster".to_string(),
                }],
            }],
            product_bundles: Vec::new(),
            related_products: Vec::new(),
            product_availabilities: vec![Availability {
                start_time_ms: 0,
                purchase_end_time_ms: 0,
                display_end_time_ms: 0,
            }],
            rule_ids: Vec::new(),
            term_type_id: 0,
            term_duration: 0,
            quantity_term_type_id: 0,
            quantity_term_duration: 0,
            is_server_validation_required: false,
            product_subscriptions: Vec::new(),
            product_type_attributes: Vec::new(),
        }
    }

    fn meta() -> MetaData {
        MetaData {
            cache_control: CacheControl {
                max_age_duration_s: 60,
            },
            server_time_ms: 0,
        }
    }

    pub fn shop_request(region_id: i64, locale: &str, placement_ids: &[&str]) -> Value {
        json!({
            "gameServiceRegionId": region_id,
            "placementIds": placement_ids,
            "storeId": 6,
            "locale": locale,
        })
    }

    pub fn balance_request(
        currency: &str,
        title_segment: &str,
        game_service_region_id: i64,
    ) -> Value {
        json!({
            "ecosystem": 0,
            "ledgerKey": {
                "gameServiceRegionId": game_service_region_id,
                "titleSegment": title_segment,
            },
            "currencyCode": currency,
        })
    }

    pub fn products_request(region_id: i64, locale: &str, product_ids: &[i64]) -> Value {
        json!({
            "paginationToken": "",
            "externalPlatformId": 0,
            "productIds": product_ids,
            "currencyCodes": [],
            "modifiedSinceFilterMs": 0,
            "gameServiceRegionId": region_id,
            "storeId": 6,
            "paginationSize": 300,
            "locale": locale,
            "requestChildProducts": false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        error_is_unspecified, fixtures, BalanceResponse, LoadProductsResponse,
        PersonalizedShopResponse,
    };

    const SPEC_XSH: &str = r#"{
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
    }"#;

    #[test]
    fn spec_xsh_example_parses() {
        let parsed: BalanceResponse = serde_json::from_str(SPEC_XSH).unwrap();
        assert!(parsed.error.is_none());
        assert!(!parsed.soft_capped);
        let ledger = parsed.balance.ledger_id;
        assert_eq!(ledger.currency_code, "XSH");
        assert_eq!(ledger.account_id, 0);
        let key = ledger.ledger_key.expect("XSH ledger key");
        assert_eq!(key.title_segment, "T5_HS_EU");
        assert!(key.game_service_region_id.is_none());
        assert_eq!(parsed.balance.purchased_balance, "0");
        assert_eq!(parsed.balance.granted_balance, "2595");
    }

    #[test]
    fn xsr_ledger_key_may_be_null() {
        let mut document = fixtures::balance("XSR", None, 0, "0", "100");
        let parsed: BalanceResponse = serde_json::from_value(document.clone()).unwrap();
        assert!(parsed.balance.ledger_id.ledger_key.is_none());
        assert_eq!(parsed.balance.ledger_id.currency_code, "XSR");
        document
            .as_object_mut()
            .unwrap()
            .insert("futureField".into(), serde_json::json!(true));
        let again: BalanceResponse = serde_json::from_value(document.clone()).unwrap();
        assert_eq!(again.balance.ledger_id.currency_code, "XSR");
        assert_eq!(document["futureField"], serde_json::json!(true));
    }

    #[test]
    fn shop_and_catalog_fixtures_match_the_recorded_shape() {
        let shop = fixtures::personalized_shop("placement-zh", "zhCN", 4242, "tab_id", "modes");
        let parsed: PersonalizedShopResponse = serde_json::from_value(shop.clone()).unwrap();
        assert!(parsed.error.is_none());
        assert_eq!(parsed.placements.len(), 1);
        assert_eq!(parsed.placements[0].placement_id, "placement-zh");
        let section = &parsed.placements[0].page.sections[0];
        assert_eq!(section.localization.locale, "zhCN");
        assert_eq!(section.attributes[0].section_attribute_key, "tab_id");
        assert_eq!(
            section.product_collections[0].items[0].product_collection_item_value,
            4242
        );
        assert_eq!(shop["metaData"]["cacheControl"]["maxAgeDurationS"], 60);

        let products = fixtures::load_products(
            "zhCN",
            vec![
                fixtures::priced_product(7, "zhCN", "EUR", "54.99"),
                fixtures::grant_product(8, "zhCN", "XSR", "100"),
            ],
        );
        let parsed: LoadProductsResponse = serde_json::from_value(products.clone()).unwrap();
        assert!(parsed.pagination_token.is_none());
        assert!(parsed.child_products.is_empty());
        assert_eq!(parsed.products[0].prices[0].currency_code, "EUR");
        assert_eq!(parsed.products[0].prices[0].original_price, "54.99");
        assert!(parsed.products[0]
            .prices
            .iter()
            .all(|price| price.currency_code != "XSH"));
        assert!(parsed.products[1].prices.is_empty());
        assert_eq!(
            parsed.products[1].virtual_currency_grants[0].currency_code,
            "XSR"
        );
        assert_eq!(
            parsed.products[1].licenses[0].license_attributes[0].key,
            "product"
        );
        assert_eq!(products["products"][0]["isServerValidationRequired"], false);
    }

    #[test]
    fn a_non_null_error_is_not_a_success_document() {
        assert!(!error_is_unspecified(&None));
        assert!(error_is_unspecified(&Some(serde_json::json!({"code": 1}))));
    }
}
