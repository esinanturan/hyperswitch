#[cfg(feature = "v2")]
use std::marker::PhantomData;

#[cfg(feature = "v2")]
use api_models::payments::{SessionToken, VaultSessionDetails};
#[cfg(feature = "v1")]
use common_types::primitive_wrappers::{
    AlwaysRequestExtendedAuthorization, RequestExtendedAuthorizationBool,
};
use common_utils::{
    self,
    crypto::Encryptable,
    encryption::Encryption,
    errors::CustomResult,
    ext_traits::ValueExt,
    id_type, pii,
    types::{keymanager::ToEncryptable, CreatedBy, MinorUnit},
};
use diesel_models::payment_intent::TaxDetails;
#[cfg(feature = "v2")]
use error_stack::ResultExt;
use masking::Secret;
use router_derive::ToEncryption;
use rustc_hash::FxHashMap;
use serde_json::Value;
use time::PrimitiveDateTime;

pub mod payment_attempt;
pub mod payment_intent;

use common_enums as storage_enums;
#[cfg(feature = "v2")]
use diesel_models::types::{FeatureMetadata, OrderDetailsWithAmount};

use self::payment_attempt::PaymentAttempt;
#[cfg(feature = "v2")]
use crate::{
    address::Address, business_profile, customer, errors, merchant_connector_account,
    merchant_connector_account::MerchantConnectorAccountTypeDetails, merchant_context,
    payment_address, payment_method_data, payment_methods, revenue_recovery, routing,
    ApiModelToDieselModelConvertor,
};
#[cfg(feature = "v1")]
use crate::{payment_method_data, RemoteStorageObject};

#[cfg(feature = "v1")]
#[derive(Clone, Debug, PartialEq, serde::Serialize, ToEncryption)]
pub struct PaymentIntent {
    pub payment_id: id_type::PaymentId,
    pub merchant_id: id_type::MerchantId,
    pub status: storage_enums::IntentStatus,
    pub amount: MinorUnit,
    pub shipping_cost: Option<MinorUnit>,
    pub currency: Option<storage_enums::Currency>,
    pub amount_captured: Option<MinorUnit>,
    pub customer_id: Option<id_type::CustomerId>,
    pub description: Option<String>,
    pub return_url: Option<String>,
    pub metadata: Option<Value>,
    pub connector_id: Option<String>,
    pub shipping_address_id: Option<String>,
    pub billing_address_id: Option<String>,
    pub statement_descriptor_name: Option<String>,
    pub statement_descriptor_suffix: Option<String>,
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub created_at: PrimitiveDateTime,
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub modified_at: PrimitiveDateTime,
    #[serde(with = "common_utils::custom_serde::iso8601::option")]
    pub last_synced: Option<PrimitiveDateTime>,
    pub setup_future_usage: Option<storage_enums::FutureUsage>,
    pub off_session: Option<bool>,
    pub client_secret: Option<String>,
    pub active_attempt: RemoteStorageObject<PaymentAttempt>,
    pub business_country: Option<storage_enums::CountryAlpha2>,
    pub business_label: Option<String>,
    pub order_details: Option<Vec<pii::SecretSerdeValue>>,
    pub allowed_payment_method_types: Option<Value>,
    pub connector_metadata: Option<Value>,
    pub feature_metadata: Option<Value>,
    pub attempt_count: i16,
    pub profile_id: Option<id_type::ProfileId>,
    pub payment_link_id: Option<String>,
    // Denotes the action(approve or reject) taken by merchant in case of manual review.
    // Manual review can occur when the transaction is marked as risky by the frm_processor, payment processor or when there is underpayment/over payment incase of crypto payment
    pub merchant_decision: Option<String>,
    pub payment_confirm_source: Option<storage_enums::PaymentSource>,

    pub updated_by: String,
    pub surcharge_applicable: Option<bool>,
    pub request_incremental_authorization: Option<storage_enums::RequestIncrementalAuthorization>,
    pub incremental_authorization_allowed: Option<bool>,
    pub authorization_count: Option<i32>,
    pub fingerprint_id: Option<String>,
    #[serde(with = "common_utils::custom_serde::iso8601::option")]
    pub session_expiry: Option<PrimitiveDateTime>,
    pub request_external_three_ds_authentication: Option<bool>,
    pub split_payments: Option<common_types::payments::SplitPaymentsRequest>,
    pub frm_metadata: Option<pii::SecretSerdeValue>,
    #[encrypt]
    pub customer_details: Option<Encryptable<Secret<Value>>>,
    #[encrypt]
    pub billing_details: Option<Encryptable<Secret<Value>>>,
    pub merchant_order_reference_id: Option<String>,
    #[encrypt]
    pub shipping_details: Option<Encryptable<Secret<Value>>>,
    pub is_payment_processor_token_flow: Option<bool>,
    pub organization_id: id_type::OrganizationId,
    pub tax_details: Option<TaxDetails>,
    pub skip_external_tax_calculation: Option<bool>,
    pub request_extended_authorization: Option<RequestExtendedAuthorizationBool>,
    pub psd2_sca_exemption_type: Option<storage_enums::ScaExemptionType>,
    pub processor_merchant_id: id_type::MerchantId,
    pub created_by: Option<CreatedBy>,
    pub force_3ds_challenge: Option<bool>,
    pub force_3ds_challenge_trigger: Option<bool>,
    pub is_iframe_redirection_enabled: Option<bool>,
    pub is_payment_id_from_merchant: Option<bool>,
    pub payment_channel: Option<common_enums::PaymentChannel>,
}

impl PaymentIntent {
    #[cfg(feature = "v1")]
    pub fn get_id(&self) -> &id_type::PaymentId {
        &self.payment_id
    }

    #[cfg(feature = "v2")]
    pub fn get_id(&self) -> &id_type::GlobalPaymentId {
        &self.id
    }

    #[cfg(feature = "v2")]
    /// This is the url to which the customer will be redirected to, to complete the redirection flow
    pub fn create_start_redirection_url(
        &self,
        base_url: &str,
        publishable_key: String,
    ) -> CustomResult<url::Url, errors::api_error_response::ApiErrorResponse> {
        let start_redirection_url = &format!(
            "{}/v2/payments/{}/start-redirection?publishable_key={}&profile_id={}",
            base_url,
            self.get_id().get_string_repr(),
            publishable_key,
            self.profile_id.get_string_repr()
        );
        url::Url::parse(start_redirection_url)
            .change_context(errors::api_error_response::ApiErrorResponse::InternalServerError)
            .attach_printable("Error creating start redirection url")
    }
    #[cfg(feature = "v1")]
    pub fn get_request_extended_authorization_bool_if_connector_supports(
        &self,
        connector: common_enums::connector_enums::Connector,
        always_request_extended_authorization_optional: Option<AlwaysRequestExtendedAuthorization>,
        payment_method_optional: Option<common_enums::PaymentMethod>,
        payment_method_type_optional: Option<common_enums::PaymentMethodType>,
    ) -> Option<RequestExtendedAuthorizationBool> {
        use router_env::logger;

        let is_extended_authorization_supported_by_connector = || {
            let supported_pms = connector.get_payment_methods_supporting_extended_authorization();
            let supported_pmts =
                connector.get_payment_method_types_supporting_extended_authorization();
            // check if payment method or payment method type is supported by the connector
            logger::info!(
                "Extended Authentication Connector:{:?}, Supported payment methods: {:?}, Supported payment method types: {:?}, Payment method Selected: {:?}, Payment method type Selected: {:?}",
                connector,
                supported_pms,
                supported_pmts,
                payment_method_optional,
                payment_method_type_optional
            );
            match (payment_method_optional, payment_method_type_optional) {
                (Some(payment_method), Some(payment_method_type)) => {
                    supported_pms.contains(&payment_method)
                        && supported_pmts.contains(&payment_method_type)
                }
                (Some(payment_method), None) => supported_pms.contains(&payment_method),
                (None, Some(payment_method_type)) => supported_pmts.contains(&payment_method_type),
                (None, None) => false,
            }
        };
        let intent_request_extended_authorization_optional = self.request_extended_authorization;
        if always_request_extended_authorization_optional.is_some_and(
            |always_request_extended_authorization| *always_request_extended_authorization,
        ) || intent_request_extended_authorization_optional.is_some_and(
            |intent_request_extended_authorization| *intent_request_extended_authorization,
        ) {
            Some(is_extended_authorization_supported_by_connector())
        } else {
            None
        }
        .map(RequestExtendedAuthorizationBool::from)
    }

    #[cfg(feature = "v2")]
    /// This is the url to which the customer will be redirected to, after completing the redirection flow
    pub fn create_finish_redirection_url(
        &self,
        base_url: &str,
        publishable_key: &str,
    ) -> CustomResult<url::Url, errors::api_error_response::ApiErrorResponse> {
        let finish_redirection_url = format!(
            "{base_url}/v2/payments/{}/finish-redirection/{publishable_key}/{}",
            self.id.get_string_repr(),
            self.profile_id.get_string_repr()
        );

        url::Url::parse(&finish_redirection_url)
            .change_context(errors::api_error_response::ApiErrorResponse::InternalServerError)
            .attach_printable("Error creating finish redirection url")
    }

    pub fn parse_and_get_metadata<T>(
        &self,
        type_name: &'static str,
    ) -> CustomResult<Option<T>, common_utils::errors::ParsingError>
    where
        T: for<'de> masking::Deserialize<'de>,
    {
        self.metadata
            .clone()
            .map(|metadata| metadata.parse_value(type_name))
            .transpose()
    }

    #[cfg(feature = "v1")]
    pub fn merge_metadata(
        &self,
        request_metadata: Value,
    ) -> Result<Value, common_utils::errors::ParsingError> {
        if !request_metadata.is_null() {
            match (&self.metadata, &request_metadata) {
                (Some(Value::Object(existing_map)), Value::Object(req_map)) => {
                    let mut merged = existing_map.clone();
                    merged.extend(req_map.clone());
                    Ok(Value::Object(merged))
                }
                (None, Value::Object(_)) => Ok(request_metadata),
                _ => {
                    router_env::logger::error!(
                        "Expected metadata to be an object, got: {:?}",
                        request_metadata
                    );
                    Err(common_utils::errors::ParsingError::UnknownError)
                }
            }
        } else {
            router_env::logger::error!("Metadata does not contain any key");
            Err(common_utils::errors::ParsingError::UnknownError)
        }
    }
}

#[cfg(feature = "v2")]
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AmountDetails {
    /// The amount of the order in the lowest denomination of currency
    pub order_amount: MinorUnit,
    /// The currency of the order
    pub currency: common_enums::Currency,
    /// The shipping cost of the order. This has to be collected from the merchant
    pub shipping_cost: Option<MinorUnit>,
    /// Tax details related to the order. This will be calculated by the external tax provider
    pub tax_details: Option<TaxDetails>,
    /// The action to whether calculate tax by calling external tax provider or not
    pub skip_external_tax_calculation: common_enums::TaxCalculationOverride,
    /// The action to whether calculate surcharge or not
    pub skip_surcharge_calculation: common_enums::SurchargeCalculationOverride,
    /// The surcharge amount to be added to the order, collected from the merchant
    pub surcharge_amount: Option<MinorUnit>,
    /// tax on surcharge amount
    pub tax_on_surcharge: Option<MinorUnit>,
    /// The total amount captured for the order. This is the sum of all the captured amounts for the order.
    /// For automatic captures, this will be the same as net amount for the order
    pub amount_captured: Option<MinorUnit>,
}

#[cfg(feature = "v2")]
impl AmountDetails {
    /// Get the action to whether calculate surcharge or not as a boolean value
    fn get_surcharge_action_as_bool(&self) -> bool {
        self.skip_surcharge_calculation.as_bool()
    }

    /// Get the action to whether calculate external tax or not as a boolean value
    pub fn get_external_tax_action_as_bool(&self) -> bool {
        self.skip_external_tax_calculation.as_bool()
    }

    /// Calculate the net amount for the order
    pub fn calculate_net_amount(&self) -> MinorUnit {
        self.order_amount
            + self.shipping_cost.unwrap_or(MinorUnit::zero())
            + self.surcharge_amount.unwrap_or(MinorUnit::zero())
            + self.tax_on_surcharge.unwrap_or(MinorUnit::zero())
    }

    pub fn create_attempt_amount_details(
        &self,
        confirm_intent_request: &api_models::payments::PaymentsConfirmIntentRequest,
    ) -> payment_attempt::AttemptAmountDetails {
        let net_amount = self.calculate_net_amount();

        let surcharge_amount = match self.skip_surcharge_calculation {
            common_enums::SurchargeCalculationOverride::Skip => self.surcharge_amount,
            common_enums::SurchargeCalculationOverride::Calculate => None,
        };

        let tax_on_surcharge = match self.skip_surcharge_calculation {
            common_enums::SurchargeCalculationOverride::Skip => self.tax_on_surcharge,
            common_enums::SurchargeCalculationOverride::Calculate => None,
        };

        let order_tax_amount = match self.skip_external_tax_calculation {
            common_enums::TaxCalculationOverride::Skip => {
                self.tax_details.as_ref().and_then(|tax_details| {
                    tax_details.get_tax_amount(Some(confirm_intent_request.payment_method_subtype))
                })
            }
            common_enums::TaxCalculationOverride::Calculate => None,
        };

        payment_attempt::AttemptAmountDetails::from(payment_attempt::AttemptAmountDetailsSetter {
            net_amount,
            amount_to_capture: None,
            surcharge_amount,
            tax_on_surcharge,
            // This will be updated when we receive response from the connector
            amount_capturable: MinorUnit::zero(),
            shipping_cost: self.shipping_cost,
            order_tax_amount,
        })
    }

    pub fn proxy_create_attempt_amount_details(
        &self,
        _confirm_intent_request: &api_models::payments::ProxyPaymentsRequest,
    ) -> payment_attempt::AttemptAmountDetails {
        let net_amount = self.calculate_net_amount();

        let surcharge_amount = match self.skip_surcharge_calculation {
            common_enums::SurchargeCalculationOverride::Skip => self.surcharge_amount,
            common_enums::SurchargeCalculationOverride::Calculate => None,
        };

        let tax_on_surcharge = match self.skip_surcharge_calculation {
            common_enums::SurchargeCalculationOverride::Skip => self.tax_on_surcharge,
            common_enums::SurchargeCalculationOverride::Calculate => None,
        };

        let order_tax_amount = match self.skip_external_tax_calculation {
            common_enums::TaxCalculationOverride::Skip => self
                .tax_details
                .as_ref()
                .and_then(|tax_details| tax_details.get_tax_amount(None)),
            common_enums::TaxCalculationOverride::Calculate => None,
        };

        payment_attempt::AttemptAmountDetails::from(payment_attempt::AttemptAmountDetailsSetter {
            net_amount,
            amount_to_capture: None,
            surcharge_amount,
            tax_on_surcharge,
            // This will be updated when we receive response from the connector
            amount_capturable: MinorUnit::zero(),
            shipping_cost: self.shipping_cost,
            order_tax_amount,
        })
    }

    pub fn update_from_request(self, req: &api_models::payments::AmountDetailsUpdate) -> Self {
        Self {
            order_amount: req
                .order_amount()
                .unwrap_or(self.order_amount.into())
                .into(),
            currency: req.currency().unwrap_or(self.currency),
            shipping_cost: req.shipping_cost().or(self.shipping_cost),
            tax_details: req
                .order_tax_amount()
                .map(|order_tax_amount| TaxDetails {
                    default: Some(diesel_models::DefaultTax { order_tax_amount }),
                    payment_method_type: None,
                })
                .or(self.tax_details),
            skip_external_tax_calculation: req
                .skip_external_tax_calculation()
                .unwrap_or(self.skip_external_tax_calculation),
            skip_surcharge_calculation: req
                .skip_surcharge_calculation()
                .unwrap_or(self.skip_surcharge_calculation),
            surcharge_amount: req.surcharge_amount().or(self.surcharge_amount),
            tax_on_surcharge: req.tax_on_surcharge().or(self.tax_on_surcharge),
            amount_captured: self.amount_captured,
        }
    }
}

#[cfg(feature = "v2")]
#[derive(Clone, Debug, PartialEq, serde::Serialize, ToEncryption)]
pub struct PaymentIntent {
    /// The global identifier for the payment intent. This is generated by the system.
    /// The format of the global id is `{cell_id:5}_pay_{time_ordered_uuid:32}`.
    pub id: id_type::GlobalPaymentId,
    /// The identifier for the merchant. This is automatically derived from the api key used to create the payment.
    pub merchant_id: id_type::MerchantId,
    /// The status of payment intent.
    pub status: storage_enums::IntentStatus,
    /// The amount related details of the payment
    pub amount_details: AmountDetails,
    /// The total amount captured for the order. This is the sum of all the captured amounts for the order.
    pub amount_captured: Option<MinorUnit>,
    /// The identifier for the customer. This is the identifier for the customer in the merchant's system.
    pub customer_id: Option<id_type::GlobalCustomerId>,
    /// The description of the order. This will be passed to connectors which support description.
    pub description: Option<common_utils::types::Description>,
    /// The return url for the payment. This is the url to which the user will be redirected after the payment is completed.
    pub return_url: Option<common_utils::types::Url>,
    /// The metadata for the payment intent. This is the metadata that will be passed to the connectors.
    pub metadata: Option<pii::SecretSerdeValue>,
    /// The statement descriptor for the order, this will be displayed in the user's bank statement.
    pub statement_descriptor: Option<common_utils::types::StatementDescriptor>,
    /// The time at which the order was created
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub created_at: PrimitiveDateTime,
    /// The time at which the order was last modified
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub modified_at: PrimitiveDateTime,
    #[serde(with = "common_utils::custom_serde::iso8601::option")]
    pub last_synced: Option<PrimitiveDateTime>,
    pub setup_future_usage: storage_enums::FutureUsage,
    /// The active attempt for the payment intent. This is the payment attempt that is currently active for the payment intent.
    pub active_attempt_id: Option<id_type::GlobalAttemptId>,
    /// The order details for the payment.
    pub order_details: Option<Vec<Secret<OrderDetailsWithAmount>>>,
    /// This is the list of payment method types that are allowed for the payment intent.
    /// This field allows the merchant to restrict the payment methods that can be used for the payment intent.
    pub allowed_payment_method_types: Option<Vec<common_enums::PaymentMethodType>>,
    /// This metadata contains details about
    pub connector_metadata: Option<pii::SecretSerdeValue>,
    pub feature_metadata: Option<FeatureMetadata>,
    /// Number of attempts that have been made for the order
    pub attempt_count: i16,
    /// The profile id for the payment.
    pub profile_id: id_type::ProfileId,
    /// The payment link id for the payment. This is generated only if `enable_payment_link` is set to true.
    pub payment_link_id: Option<String>,
    /// This Denotes the action(approve or reject) taken by merchant in case of manual review.
    /// Manual review can occur when the transaction is marked as risky by the frm_processor, payment processor or when there is underpayment/over payment incase of crypto payment
    pub frm_merchant_decision: Option<common_enums::MerchantDecision>,
    /// Denotes the last instance which updated the payment
    pub updated_by: String,
    /// Denotes whether merchant requested for incremental authorization to be enabled for this payment.
    pub request_incremental_authorization: storage_enums::RequestIncrementalAuthorization,
    /// Denotes the number of authorizations that have been made for the payment.
    pub authorization_count: Option<i32>,
    /// Denotes the client secret expiry for the payment. This is the time at which the client secret will expire.
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub session_expiry: PrimitiveDateTime,
    /// Denotes whether merchant requested for 3ds authentication to be enabled for this payment.
    pub request_external_three_ds_authentication: common_enums::External3dsAuthenticationRequest,
    /// Metadata related to fraud and risk management
    pub frm_metadata: Option<pii::SecretSerdeValue>,
    /// The details of the customer in a denormalized form. Only a subset of fields are stored.
    #[encrypt]
    pub customer_details: Option<Encryptable<Secret<Value>>>,
    /// The reference id for the order in the merchant's system. This value can be passed by the merchant.
    pub merchant_reference_id: Option<id_type::PaymentReferenceId>,
    /// The billing address for the order in a denormalized form.
    #[encrypt(ty = Value)]
    pub billing_address: Option<Encryptable<Address>>,
    /// The shipping address for the order in a denormalized form.
    #[encrypt(ty = Value)]
    pub shipping_address: Option<Encryptable<Address>>,
    /// Capture method for the payment
    pub capture_method: storage_enums::CaptureMethod,
    /// Authentication type that is requested by the merchant for this payment.
    pub authentication_type: Option<common_enums::AuthenticationType>,
    /// This contains the pre routing results that are done when routing is done during listing the payment methods.
    pub prerouting_algorithm: Option<routing::PaymentRoutingInfo>,
    /// The organization id for the payment. This is derived from the merchant account
    pub organization_id: id_type::OrganizationId,
    /// Denotes the request by the merchant whether to enable a payment link for this payment.
    pub enable_payment_link: common_enums::EnablePaymentLinkRequest,
    /// Denotes the request by the merchant whether to apply MIT exemption for this payment
    pub apply_mit_exemption: common_enums::MitExemptionRequest,
    /// Denotes whether the customer is present during the payment flow. This information may be used for 3ds authentication
    pub customer_present: common_enums::PresenceOfCustomerDuringPayment,
    /// Denotes the override for payment link configuration
    pub payment_link_config: Option<diesel_models::PaymentLinkConfigRequestForPayments>,
    /// The straight through routing algorithm id that is used for this payment. This overrides the default routing algorithm that is configured in business profile.
    pub routing_algorithm_id: Option<id_type::RoutingId>,
    /// Split Payment Data
    pub split_payments: Option<common_types::payments::SplitPaymentsRequest>,

    pub force_3ds_challenge: Option<bool>,
    pub force_3ds_challenge_trigger: Option<bool>,
    /// merchant who owns the credentials of the processor, i.e. processor owner
    pub processor_merchant_id: id_type::MerchantId,
    /// merchantwho invoked the resource based api (identifier) and through what source (Api, Jwt(Dashboard))
    pub created_by: Option<CreatedBy>,

    /// Indicates if the redirection has to open in the iframe
    pub is_iframe_redirection_enabled: Option<bool>,

    /// Indicates whether the payment_id was provided by the merchant (true),
    /// or generated internally by Hyperswitch (false)
    pub is_payment_id_from_merchant: Option<bool>,
}

#[cfg(feature = "v2")]
impl PaymentIntent {
    fn get_payment_method_sub_type(&self) -> Option<common_enums::PaymentMethodType> {
        self.feature_metadata
            .as_ref()
            .and_then(|metadata| metadata.get_payment_method_sub_type())
    }

    fn get_payment_method_type(&self) -> Option<common_enums::PaymentMethod> {
        self.feature_metadata
            .as_ref()
            .and_then(|metadata| metadata.get_payment_method_type())
    }

    pub fn get_connector_customer_id_from_feature_metadata(&self) -> Option<String> {
        self.feature_metadata
            .as_ref()
            .and_then(|metadata| metadata.payment_revenue_recovery_metadata.as_ref())
            .map(|recovery_metadata| {
                recovery_metadata
                    .billing_connector_payment_details
                    .connector_customer_id
                    .clone()
            })
    }

    pub fn get_billing_merchant_connector_account_id(
        &self,
    ) -> Option<id_type::MerchantConnectorAccountId> {
        self.feature_metadata.as_ref().and_then(|feature_metadata| {
            feature_metadata.get_billing_merchant_connector_account_id()
        })
    }

    fn get_request_incremental_authorization_value(
        request: &api_models::payments::PaymentsCreateIntentRequest,
    ) -> CustomResult<
        common_enums::RequestIncrementalAuthorization,
        errors::api_error_response::ApiErrorResponse,
    > {
        request.request_incremental_authorization
            .map(|request_incremental_authorization| {
                if request_incremental_authorization == common_enums::RequestIncrementalAuthorization::True {
                    if request.capture_method == Some(common_enums::CaptureMethod::Automatic) {
                        Err(errors::api_error_response::ApiErrorResponse::InvalidRequestData { message: "incremental authorization is not supported when capture_method is automatic".to_owned() })?
                    }
                    Ok(common_enums::RequestIncrementalAuthorization::True)
                } else {
                    Ok(common_enums::RequestIncrementalAuthorization::False)
                }
            })
            .unwrap_or(Ok(common_enums::RequestIncrementalAuthorization::default()))
    }

    pub async fn create_domain_model_from_request(
        payment_id: &id_type::GlobalPaymentId,
        merchant_context: &merchant_context::MerchantContext,
        profile: &business_profile::Profile,
        request: api_models::payments::PaymentsCreateIntentRequest,
        decrypted_payment_intent: DecryptedPaymentIntent,
    ) -> CustomResult<Self, errors::api_error_response::ApiErrorResponse> {
        let connector_metadata = request
            .get_connector_metadata_as_value()
            .change_context(errors::api_error_response::ApiErrorResponse::InternalServerError)
            .attach_printable("Error getting connector metadata as value")?;
        let request_incremental_authorization =
            Self::get_request_incremental_authorization_value(&request)?;
        let allowed_payment_method_types = request.allowed_payment_method_types;

        let session_expiry =
            common_utils::date_time::now().saturating_add(time::Duration::seconds(
                request.session_expiry.map(i64::from).unwrap_or(
                    profile
                        .session_expiry
                        .unwrap_or(common_utils::consts::DEFAULT_SESSION_EXPIRY),
                ),
            ));
        let order_details = request.order_details.map(|order_details| {
            order_details
                .into_iter()
                .map(|order_detail| Secret::new(OrderDetailsWithAmount::convert_from(order_detail)))
                .collect()
        });

        Ok(Self {
            id: payment_id.clone(),
            merchant_id: merchant_context.get_merchant_account().get_id().clone(),
            // Intent status would be RequiresPaymentMethod because we are creating a new payment intent
            status: common_enums::IntentStatus::RequiresPaymentMethod,
            amount_details: AmountDetails::from(request.amount_details),
            amount_captured: None,
            customer_id: request.customer_id,
            description: request.description,
            return_url: request.return_url,
            metadata: request.metadata,
            statement_descriptor: request.statement_descriptor,
            created_at: common_utils::date_time::now(),
            modified_at: common_utils::date_time::now(),
            last_synced: None,
            setup_future_usage: request.setup_future_usage.unwrap_or_default(),
            active_attempt_id: None,
            order_details,
            allowed_payment_method_types,
            connector_metadata,
            feature_metadata: request.feature_metadata.map(FeatureMetadata::convert_from),
            // Attempt count is 0 in create intent as no attempt is made yet
            attempt_count: 0,
            profile_id: profile.get_id().clone(),
            payment_link_id: None,
            frm_merchant_decision: None,
            updated_by: merchant_context
                .get_merchant_account()
                .storage_scheme
                .to_string(),
            request_incremental_authorization,
            // Authorization count is 0 in create intent as no authorization is made yet
            authorization_count: Some(0),
            session_expiry,
            request_external_three_ds_authentication: request
                .request_external_three_ds_authentication
                .unwrap_or_default(),
            frm_metadata: request.frm_metadata,
            customer_details: None,
            merchant_reference_id: request.merchant_reference_id,
            billing_address: decrypted_payment_intent
                .billing_address
                .as_ref()
                .map(|data| {
                    data.clone()
                        .deserialize_inner_value(|value| value.parse_value("Address"))
                })
                .transpose()
                .change_context(errors::api_error_response::ApiErrorResponse::InternalServerError)
                .attach_printable("Unable to decode billing address")?,
            shipping_address: decrypted_payment_intent
                .shipping_address
                .as_ref()
                .map(|data| {
                    data.clone()
                        .deserialize_inner_value(|value| value.parse_value("Address"))
                })
                .transpose()
                .change_context(errors::api_error_response::ApiErrorResponse::InternalServerError)
                .attach_printable("Unable to decode shipping address")?,
            capture_method: request.capture_method.unwrap_or_default(),
            authentication_type: request.authentication_type,
            prerouting_algorithm: None,
            organization_id: merchant_context
                .get_merchant_account()
                .organization_id
                .clone(),
            enable_payment_link: request.payment_link_enabled.unwrap_or_default(),
            apply_mit_exemption: request.apply_mit_exemption.unwrap_or_default(),
            customer_present: request.customer_present.unwrap_or_default(),
            payment_link_config: request
                .payment_link_config
                .map(ApiModelToDieselModelConvertor::convert_from),
            routing_algorithm_id: request.routing_algorithm_id,
            split_payments: None,
            force_3ds_challenge: None,
            force_3ds_challenge_trigger: None,
            processor_merchant_id: merchant_context.get_merchant_account().get_id().clone(),
            created_by: None,
            is_iframe_redirection_enabled: None,
            is_payment_id_from_merchant: None,
        })
    }

    pub fn get_revenue_recovery_metadata(
        &self,
    ) -> Option<diesel_models::types::PaymentRevenueRecoveryMetadata> {
        self.feature_metadata
            .as_ref()
            .and_then(|feature_metadata| feature_metadata.payment_revenue_recovery_metadata.clone())
    }

    pub fn get_feature_metadata(&self) -> Option<FeatureMetadata> {
        self.feature_metadata.clone()
    }

    pub fn create_revenue_recovery_attempt_data(
        &self,
        revenue_recovery_metadata: api_models::payments::PaymentRevenueRecoveryMetadata,
        billing_connector_account: &merchant_connector_account::MerchantConnectorAccount,
    ) -> CustomResult<
        revenue_recovery::RevenueRecoveryAttemptData,
        errors::api_error_response::ApiErrorResponse,
    > {
        let merchant_reference_id = self.merchant_reference_id.clone().ok_or_else(|| {
            error_stack::report!(
                errors::api_error_response::ApiErrorResponse::GenericNotFoundError {
                    message: "mandate reference id not found".to_string()
                }
            )
        })?;

        let connector_account_reference_id = billing_connector_account
            .get_account_reference_id_using_payment_merchant_connector_account_id(
                revenue_recovery_metadata.active_attempt_payment_connector_id,
            )
            .ok_or_else(|| {
                error_stack::report!(
                    errors::api_error_response::ApiErrorResponse::GenericNotFoundError {
                        message: "connector account reference id not found".to_string()
                    }
                )
            })?;

        Ok(revenue_recovery::RevenueRecoveryAttemptData {
            amount: self.amount_details.order_amount,
            currency: self.amount_details.currency,
            merchant_reference_id,
            connector_transaction_id: None, // No connector id
            error_code: None,
            error_message: None,
            processor_payment_method_token: revenue_recovery_metadata
                .billing_connector_payment_details
                .payment_processor_token,
            connector_customer_id: revenue_recovery_metadata
                .billing_connector_payment_details
                .connector_customer_id,
            connector_account_reference_id,
            transaction_created_at: None, // would unwrap_or as now
            status: common_enums::AttemptStatus::Started,
            payment_method_type: self
                .get_payment_method_type()
                .unwrap_or(revenue_recovery_metadata.payment_method_type),
            payment_method_sub_type: self
                .get_payment_method_sub_type()
                .unwrap_or(revenue_recovery_metadata.payment_method_subtype),
            network_advice_code: None,
            network_decline_code: None,
            network_error_message: None,
            retry_count: None,
            invoice_next_billing_time: None,
            invoice_billing_started_at_time: None,
            card_isin: None,
            card_network: None,
            // No charge id is present here since it is an internal payment and we didn't call connector yet.
            charge_id: None,
        })
    }

    pub fn get_optional_customer_id(
        &self,
    ) -> CustomResult<Option<id_type::CustomerId>, common_utils::errors::ValidationError> {
        self.customer_id
            .as_ref()
            .map(|customer_id| id_type::CustomerId::try_from(customer_id.clone()))
            .transpose()
    }

    pub fn get_optional_connector_metadata(
        &self,
    ) -> CustomResult<
        Option<api_models::payments::ConnectorMetadata>,
        common_utils::errors::ParsingError,
    > {
        self.connector_metadata
            .clone()
            .map(|cm| {
                cm.parse_value::<api_models::payments::ConnectorMetadata>("ConnectorMetadata")
            })
            .transpose()
    }

    pub fn get_currency(&self) -> storage_enums::Currency {
        self.amount_details.currency
    }
}

#[cfg(feature = "v1")]
#[derive(Default, Debug, Clone)]
pub struct HeaderPayload {
    pub payment_confirm_source: Option<common_enums::PaymentSource>,
    pub client_source: Option<String>,
    pub client_version: Option<String>,
    pub x_hs_latency: Option<bool>,
    pub browser_name: Option<common_enums::BrowserName>,
    pub x_client_platform: Option<common_enums::ClientPlatform>,
    pub x_merchant_domain: Option<String>,
    pub locale: Option<String>,
    pub x_app_id: Option<String>,
    pub x_redirect_uri: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClickToPayMetaData {
    pub dpa_id: String,
    pub dpa_name: String,
    pub locale: String,
    pub card_brands: Vec<String>,
    pub acquirer_bin: String,
    pub acquirer_merchant_id: String,
    pub merchant_category_code: String,
    pub merchant_country_code: String,
    pub dpa_client_id: Option<String>,
}

// TODO: uncomment fields as necessary
#[cfg(feature = "v2")]
#[derive(Default, Debug, Clone)]
pub struct HeaderPayload {
    /// The source with which the payment is confirmed.
    pub payment_confirm_source: Option<common_enums::PaymentSource>,
    // pub client_source: Option<String>,
    // pub client_version: Option<String>,
    pub x_hs_latency: Option<bool>,
    pub browser_name: Option<common_enums::BrowserName>,
    pub x_client_platform: Option<common_enums::ClientPlatform>,
    pub x_merchant_domain: Option<String>,
    pub locale: Option<String>,
    pub x_app_id: Option<String>,
    pub x_redirect_uri: Option<String>,
}

impl HeaderPayload {
    pub fn with_source(payment_confirm_source: common_enums::PaymentSource) -> Self {
        Self {
            payment_confirm_source: Some(payment_confirm_source),
            ..Default::default()
        }
    }
}

#[cfg(feature = "v2")]
#[derive(Clone)]
pub struct PaymentIntentData<F>
where
    F: Clone,
{
    pub flow: PhantomData<F>,
    pub payment_intent: PaymentIntent,
    pub sessions_token: Vec<SessionToken>,
    pub client_secret: Option<Secret<String>>,
    pub vault_session_details: Option<VaultSessionDetails>,
    pub connector_customer_id: Option<String>,
}

#[cfg(feature = "v2")]
#[derive(Clone)]
pub struct PaymentAttemptListData<F>
where
    F: Clone,
{
    pub flow: PhantomData<F>,
    pub payment_attempt_list: Vec<PaymentAttempt>,
}

// TODO: Check if this can be merged with existing payment data
#[cfg(feature = "v2")]
#[derive(Clone, Debug)]
pub struct PaymentConfirmData<F>
where
    F: Clone,
{
    pub flow: PhantomData<F>,
    pub payment_intent: PaymentIntent,
    pub payment_attempt: PaymentAttempt,
    pub payment_method_data: Option<payment_method_data::PaymentMethodData>,
    pub payment_address: payment_address::PaymentAddress,
    pub mandate_data: Option<api_models::payments::MandateIds>,
    pub payment_method: Option<payment_methods::PaymentMethod>,
    pub merchant_connector_details: Option<common_types::domain::MerchantConnectorAuthDetails>,
}

#[cfg(feature = "v2")]
impl<F: Clone> PaymentConfirmData<F> {
    pub fn get_connector_customer_id(
        &self,
        customer: Option<&customer::Customer>,
        merchant_connector_account: &MerchantConnectorAccountTypeDetails,
    ) -> Option<String> {
        match merchant_connector_account {
            MerchantConnectorAccountTypeDetails::MerchantConnectorAccount(_) => customer
                .and_then(|customer| customer.get_connector_customer_id(merchant_connector_account))
                .map(|id| id.to_string())
                .or_else(|| {
                    self.payment_intent
                        .get_connector_customer_id_from_feature_metadata()
                }),
            MerchantConnectorAccountTypeDetails::MerchantConnectorDetails(_) => None,
        }
    }

    pub fn update_payment_method_data(
        &mut self,
        payment_method_data: payment_method_data::PaymentMethodData,
    ) {
        self.payment_method_data = Some(payment_method_data);
    }

    pub fn update_payment_method_and_pm_id(
        &mut self,
        payment_method_id: id_type::GlobalPaymentMethodId,
        payment_method: payment_methods::PaymentMethod,
    ) {
        self.payment_attempt.payment_method_id = Some(payment_method_id);
        self.payment_method = Some(payment_method);
    }
}

#[cfg(feature = "v2")]
#[derive(Clone)]
pub struct PaymentStatusData<F>
where
    F: Clone,
{
    pub flow: PhantomData<F>,
    pub payment_intent: PaymentIntent,
    pub payment_attempt: PaymentAttempt,
    pub payment_address: payment_address::PaymentAddress,
    pub attempts: Option<Vec<PaymentAttempt>>,
    /// Should the payment status be synced with connector
    /// This will depend on the payment status and the force sync flag in the request
    pub should_sync_with_connector: bool,
    pub merchant_connector_details: Option<common_types::domain::MerchantConnectorAuthDetails>,
}

#[cfg(feature = "v2")]
#[derive(Clone)]
pub struct PaymentCaptureData<F>
where
    F: Clone,
{
    pub flow: PhantomData<F>,
    pub payment_intent: PaymentIntent,
    pub payment_attempt: PaymentAttempt,
}

#[cfg(feature = "v2")]
impl<F> PaymentStatusData<F>
where
    F: Clone,
{
    pub fn get_payment_id(&self) -> &id_type::GlobalPaymentId {
        &self.payment_intent.id
    }
}

#[cfg(feature = "v2")]
#[derive(Clone)]
pub struct PaymentAttemptRecordData<F>
where
    F: Clone,
{
    pub flow: PhantomData<F>,
    pub payment_intent: PaymentIntent,
    pub payment_attempt: PaymentAttempt,
    pub revenue_recovery_data: RevenueRecoveryData,
    pub payment_address: payment_address::PaymentAddress,
}

#[cfg(feature = "v2")]
#[derive(Clone)]
pub struct RevenueRecoveryData {
    pub billing_connector_id: id_type::MerchantConnectorAccountId,
    pub processor_payment_method_token: String,
    pub connector_customer_id: String,
    pub retry_count: Option<u16>,
    pub invoice_next_billing_time: Option<PrimitiveDateTime>,
    pub triggered_by: storage_enums::enums::TriggeredBy,
    pub card_network: Option<common_enums::CardNetwork>,
    pub card_issuer: Option<String>,
}

#[cfg(feature = "v2")]
impl<F> PaymentAttemptRecordData<F>
where
    F: Clone,
{
    pub fn get_updated_feature_metadata(
        &self,
    ) -> CustomResult<Option<FeatureMetadata>, errors::api_error_response::ApiErrorResponse> {
        let payment_intent_feature_metadata = self.payment_intent.get_feature_metadata();
        let revenue_recovery = self.payment_intent.get_revenue_recovery_metadata();
        let payment_attempt_connector = self.payment_attempt.connector.clone();

        let feature_metadata_first_pg_error_code = revenue_recovery
            .as_ref()
            .and_then(|data| data.first_payment_attempt_pg_error_code.clone());

        let (first_pg_error_code, first_network_advice_code, first_network_decline_code) =
            feature_metadata_first_pg_error_code.map_or_else(
                || {
                    let first_pg_error_code = self
                        .payment_attempt
                        .error
                        .as_ref()
                        .map(|error| error.code.clone());
                    let first_network_advice_code = self
                        .payment_attempt
                        .error
                        .as_ref()
                        .and_then(|error| error.network_advice_code.clone());
                    let first_network_decline_code = self
                        .payment_attempt
                        .error
                        .as_ref()
                        .and_then(|error| error.network_decline_code.clone());
                    (
                        first_pg_error_code,
                        first_network_advice_code,
                        first_network_decline_code,
                    )
                },
                |pg_code| {
                    let advice_code = revenue_recovery
                        .as_ref()
                        .and_then(|data| data.first_payment_attempt_network_advice_code.clone());
                    let decline_code = revenue_recovery
                        .as_ref()
                        .and_then(|data| data.first_payment_attempt_network_decline_code.clone());
                    (Some(pg_code), advice_code, decline_code)
                },
            );

        let billing_connector_payment_method_details = Some(
            diesel_models::types::BillingConnectorPaymentMethodDetails::Card(
                diesel_models::types::BillingConnectorAdditionalCardInfo {
                    card_network: self.revenue_recovery_data.card_network.clone(),
                    card_issuer: self.revenue_recovery_data.card_issuer.clone(),
                },
            ),
        );

        let payment_revenue_recovery_metadata = match payment_attempt_connector {
            Some(connector) => Some(diesel_models::types::PaymentRevenueRecoveryMetadata {
                // Update retry count by one.
                total_retry_count: revenue_recovery.as_ref().map_or(
                    self.revenue_recovery_data
                        .retry_count
                        .map_or_else(|| 1, |retry_count| retry_count),
                    |data| (data.total_retry_count + 1),
                ),
                // Since this is an external system call, marking this payment_connector_transmission to ConnectorCallSucceeded.
                payment_connector_transmission:
                    common_enums::PaymentConnectorTransmission::ConnectorCallUnsuccessful,
                billing_connector_id: self.revenue_recovery_data.billing_connector_id.clone(),
                active_attempt_payment_connector_id: self
                    .payment_attempt
                    .get_attempt_merchant_connector_account_id()?,
                billing_connector_payment_details:
                    diesel_models::types::BillingConnectorPaymentDetails {
                        payment_processor_token: self
                            .revenue_recovery_data
                            .processor_payment_method_token
                            .clone(),
                        connector_customer_id: self
                            .revenue_recovery_data
                            .connector_customer_id
                            .clone(),
                    },
                payment_method_type: self.payment_attempt.payment_method_type,
                payment_method_subtype: self.payment_attempt.payment_method_subtype,
                connector: connector.parse().map_err(|err| {
                    router_env::logger::error!(?err, "Failed to parse connector string to enum");
                    errors::api_error_response::ApiErrorResponse::InternalServerError
                })?,
                invoice_next_billing_time: self.revenue_recovery_data.invoice_next_billing_time,
                invoice_billing_started_at_time: self
                    .revenue_recovery_data
                    .invoice_next_billing_time,
                billing_connector_payment_method_details,
                first_payment_attempt_network_advice_code: first_network_advice_code,
                first_payment_attempt_network_decline_code: first_network_decline_code,
                first_payment_attempt_pg_error_code: first_pg_error_code,
            }),
            None => Err(errors::api_error_response::ApiErrorResponse::InternalServerError)
                .attach_printable("Connector not found in payment attempt")?,
        };
        Ok(Some(FeatureMetadata {
            redirect_response: payment_intent_feature_metadata
                .as_ref()
                .and_then(|data| data.redirect_response.clone()),
            search_tags: payment_intent_feature_metadata
                .as_ref()
                .and_then(|data| data.search_tags.clone()),
            apple_pay_recurring_details: payment_intent_feature_metadata
                .as_ref()
                .and_then(|data| data.apple_pay_recurring_details.clone()),
            payment_revenue_recovery_metadata,
        }))
    }
}

#[derive(Default, Clone, serde::Serialize, Debug)]
pub struct CardAndNetworkTokenDataForVault {
    pub card_data: payment_method_data::Card,
    pub network_token: NetworkTokenDataForVault,
}

#[derive(Default, Clone, serde::Serialize, Debug)]
pub struct NetworkTokenDataForVault {
    pub network_token_data: payment_method_data::NetworkTokenData,
    pub network_token_req_ref_id: String,
}

#[derive(Default, Clone, serde::Serialize, Debug)]
pub struct CardDataForVault {
    pub card_data: payment_method_data::Card,
    pub network_token_req_ref_id: Option<String>,
}

#[derive(Clone, serde::Serialize, Debug)]
pub enum VaultOperation {
    ExistingVaultData(VaultData),
    SaveCardData(CardDataForVault),
    SaveCardAndNetworkTokenData(Box<CardAndNetworkTokenDataForVault>),
}

impl VaultOperation {
    pub fn get_updated_vault_data(
        existing_vault_data: Option<&Self>,
        payment_method_data: &payment_method_data::PaymentMethodData,
    ) -> Option<Self> {
        match (existing_vault_data, payment_method_data) {
            (None, payment_method_data::PaymentMethodData::Card(card)) => {
                Some(Self::ExistingVaultData(VaultData::Card(card.clone())))
            }
            (None, payment_method_data::PaymentMethodData::NetworkToken(nt_data)) => Some(
                Self::ExistingVaultData(VaultData::NetworkToken(nt_data.clone())),
            ),
            (Some(Self::ExistingVaultData(vault_data)), payment_method_data) => {
                match (vault_data, payment_method_data) {
                    (
                        VaultData::Card(card),
                        payment_method_data::PaymentMethodData::NetworkToken(nt_data),
                    ) => Some(Self::ExistingVaultData(VaultData::CardAndNetworkToken(
                        Box::new(CardAndNetworkTokenData {
                            card_data: card.clone(),
                            network_token_data: nt_data.clone(),
                        }),
                    ))),
                    (
                        VaultData::NetworkToken(nt_data),
                        payment_method_data::PaymentMethodData::Card(card),
                    ) => Some(Self::ExistingVaultData(VaultData::CardAndNetworkToken(
                        Box::new(CardAndNetworkTokenData {
                            card_data: card.clone(),
                            network_token_data: nt_data.clone(),
                        }),
                    ))),
                    _ => Some(Self::ExistingVaultData(vault_data.clone())),
                }
            }
            //payment_method_data is not card or network token
            _ => None,
        }
    }
}

#[derive(Clone, serde::Serialize, Debug)]
pub enum VaultData {
    Card(payment_method_data::Card),
    NetworkToken(payment_method_data::NetworkTokenData),
    CardAndNetworkToken(Box<CardAndNetworkTokenData>),
}

#[derive(Default, Clone, serde::Serialize, Debug)]
pub struct CardAndNetworkTokenData {
    pub card_data: payment_method_data::Card,
    pub network_token_data: payment_method_data::NetworkTokenData,
}

impl VaultData {
    pub fn get_card_vault_data(&self) -> Option<payment_method_data::Card> {
        match self {
            Self::Card(card_data) => Some(card_data.clone()),
            Self::NetworkToken(_network_token_data) => None,
            Self::CardAndNetworkToken(vault_data) => Some(vault_data.card_data.clone()),
        }
    }

    pub fn get_network_token_data(&self) -> Option<payment_method_data::NetworkTokenData> {
        match self {
            Self::Card(_card_data) => None,
            Self::NetworkToken(network_token_data) => Some(network_token_data.clone()),
            Self::CardAndNetworkToken(vault_data) => Some(vault_data.network_token_data.clone()),
        }
    }
}
