use std::str::FromStr;

use common_enums::enums;
use common_utils::{id_type, pii::Email, request::Method, types::StringMajorUnit};
use error_stack::report;
use hyperswitch_domain_models::{
    payment_method_data::{BankRedirectData, Card, PayLaterData, PaymentMethodData, WalletData},
    router_data::{ConnectorAuthType, RouterData},
    router_request_types::{
        PaymentsAuthorizeData, PaymentsCancelData, PaymentsSyncData, ResponseId,
    },
    router_response_types::{
        MandateReference, PaymentsResponseData, RedirectForm, RefundsResponseData,
    },
    types::{
        PaymentsAuthorizeRouterData, PaymentsCancelRouterData, PaymentsCaptureRouterData,
        RefundsRouterData,
    },
};
use hyperswitch_interfaces::errors;
use masking::{ExposeInterface, Secret};
use serde::{Deserialize, Serialize};
use url::Url;

use super::aci_result_codes::{FAILURE_CODES, PENDING_CODES, SUCCESSFUL_CODES};
use crate::{
    types::{RefundsResponseRouterData, ResponseRouterData},
    utils::{self, PhoneDetailsData, RouterData as _},
};

type Error = error_stack::Report<errors::ConnectorError>;

trait GetCaptureMethod {
    fn get_capture_method(&self) -> Option<enums::CaptureMethod>;
}

impl GetCaptureMethod for PaymentsAuthorizeData {
    fn get_capture_method(&self) -> Option<enums::CaptureMethod> {
        self.capture_method
    }
}

impl GetCaptureMethod for PaymentsSyncData {
    fn get_capture_method(&self) -> Option<enums::CaptureMethod> {
        self.capture_method
    }
}

impl GetCaptureMethod for PaymentsCancelData {
    fn get_capture_method(&self) -> Option<enums::CaptureMethod> {
        None
    }
}

#[derive(Debug, Serialize)]
pub struct AciRouterData<T> {
    amount: StringMajorUnit,
    router_data: T,
}

impl<T> From<(StringMajorUnit, T)> for AciRouterData<T> {
    fn from((amount, item): (StringMajorUnit, T)) -> Self {
        Self {
            amount,
            router_data: item,
        }
    }
}

pub struct AciAuthType {
    pub api_key: Secret<String>,
    pub entity_id: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for AciAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &ConnectorAuthType) -> Result<Self, Self::Error> {
        if let ConnectorAuthType::BodyKey { api_key, key1 } = item {
            Ok(Self {
                api_key: api_key.to_owned(),
                entity_id: key1.to_owned(),
            })
        } else {
            Err(errors::ConnectorError::FailedToObtainAuthType)?
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciPaymentsRequest {
    #[serde(flatten)]
    pub txn_details: TransactionDetails,
    #[serde(flatten)]
    pub payment_method: PaymentDetails,
    #[serde(flatten)]
    pub instruction: Option<Instruction>,
    pub shopper_result_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionDetails {
    pub entity_id: Secret<String>,
    pub amount: StringMajorUnit,
    pub currency: String,
    pub payment_type: AciPaymentType,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciCancelRequest {
    pub entity_id: Secret<String>,
    pub payment_type: AciPaymentType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum PaymentDetails {
    #[serde(rename = "card")]
    AciCard(Box<CardDetails>),
    BankRedirect(Box<BankRedirectionPMData>),
    Wallet(Box<WalletPMData>),
    Klarna,
    Mandate,
}

impl TryFrom<(&WalletData, &PaymentsAuthorizeRouterData)> for PaymentDetails {
    type Error = Error;
    fn try_from(value: (&WalletData, &PaymentsAuthorizeRouterData)) -> Result<Self, Self::Error> {
        let (wallet_data, item) = value;
        let payment_data = match wallet_data {
            WalletData::MbWayRedirect(_) => {
                let phone_details = item.get_billing_phone()?;
                Self::Wallet(Box::new(WalletPMData {
                    payment_brand: PaymentBrand::Mbway,
                    account_id: Some(phone_details.get_number_with_hash_country_code()?),
                }))
            }
            WalletData::AliPayRedirect { .. } => Self::Wallet(Box::new(WalletPMData {
                payment_brand: PaymentBrand::AliPay,
                account_id: None,
            })),
            WalletData::AliPayHkRedirect(_)
            | WalletData::AmazonPayRedirect(_)
            | WalletData::Paysera(_)
            | WalletData::Skrill(_)
            | WalletData::MomoRedirect(_)
            | WalletData::KakaoPayRedirect(_)
            | WalletData::GoPayRedirect(_)
            | WalletData::GcashRedirect(_)
            | WalletData::ApplePay(_)
            | WalletData::ApplePayThirdPartySdk(_)
            | WalletData::DanaRedirect { .. }
            | WalletData::GooglePay(_)
            | WalletData::BluecodeRedirect {}
            | WalletData::GooglePayThirdPartySdk(_)
            | WalletData::MobilePayRedirect(_)
            | WalletData::PaypalRedirect(_)
            | WalletData::PaypalSdk(_)
            | WalletData::Paze(_)
            | WalletData::SamsungPay(_)
            | WalletData::TwintRedirect { .. }
            | WalletData::VippsRedirect { .. }
            | WalletData::TouchNGoRedirect(_)
            | WalletData::WeChatPayRedirect(_)
            | WalletData::WeChatPayQr(_)
            | WalletData::CashappQr(_)
            | WalletData::SwishQr(_)
            | WalletData::AliPayQr(_)
            | WalletData::ApplePayRedirect(_)
            | WalletData::GooglePayRedirect(_)
            | WalletData::Mifinity(_)
            | WalletData::RevolutPay(_) => Err(errors::ConnectorError::NotImplemented(
                "Payment method".to_string(),
            ))?,
        };
        Ok(payment_data)
    }
}

impl
    TryFrom<(
        &AciRouterData<&PaymentsAuthorizeRouterData>,
        &BankRedirectData,
    )> for PaymentDetails
{
    type Error = Error;
    fn try_from(
        value: (
            &AciRouterData<&PaymentsAuthorizeRouterData>,
            &BankRedirectData,
        ),
    ) -> Result<Self, Self::Error> {
        let (item, bank_redirect_data) = value;
        let payment_data = match bank_redirect_data {
            BankRedirectData::Eps { .. } => Self::BankRedirect(Box::new(BankRedirectionPMData {
                payment_brand: PaymentBrand::Eps,
                bank_account_country: Some(item.router_data.get_billing_country()?),
                bank_account_bank_name: None,
                bank_account_bic: None,
                bank_account_iban: None,
                billing_country: None,
                merchant_customer_id: None,
                merchant_transaction_id: None,
                customer_email: None,
            })),
            BankRedirectData::Eft { .. } => Self::BankRedirect(Box::new(BankRedirectionPMData {
                payment_brand: PaymentBrand::Eft,
                bank_account_country: Some(item.router_data.get_billing_country()?),
                bank_account_bank_name: None,
                bank_account_bic: None,
                bank_account_iban: None,
                billing_country: None,
                merchant_customer_id: None,
                merchant_transaction_id: None,
                customer_email: None,
            })),
            BankRedirectData::Giropay {
                bank_account_bic,
                bank_account_iban,
                ..
            } => Self::BankRedirect(Box::new(BankRedirectionPMData {
                payment_brand: PaymentBrand::Giropay,
                bank_account_country: Some(item.router_data.get_billing_country()?),
                bank_account_bank_name: None,
                bank_account_bic: bank_account_bic.clone(),
                bank_account_iban: bank_account_iban.clone(),
                billing_country: None,
                merchant_customer_id: None,
                merchant_transaction_id: None,
                customer_email: None,
            })),
            BankRedirectData::Ideal { bank_name, .. } => {
                Self::BankRedirect(Box::new(BankRedirectionPMData {
                    payment_brand: PaymentBrand::Ideal,
                    bank_account_country: Some(item.router_data.get_billing_country()?),
                    bank_account_bank_name: Some(bank_name.ok_or(
                        errors::ConnectorError::MissingRequiredField {
                            field_name: "ideal.bank_name",
                        },
                    )?),
                    bank_account_bic: None,
                    bank_account_iban: None,
                    billing_country: None,
                    merchant_customer_id: None,
                    merchant_transaction_id: None,
                    customer_email: None,
                }))
            }
            BankRedirectData::Sofort { .. } => {
                Self::BankRedirect(Box::new(BankRedirectionPMData {
                    payment_brand: PaymentBrand::Sofortueberweisung,
                    bank_account_country: Some(item.router_data.get_billing_country()?),
                    bank_account_bank_name: None,
                    bank_account_bic: None,
                    bank_account_iban: None,
                    billing_country: None,
                    merchant_customer_id: None,
                    merchant_transaction_id: None,
                    customer_email: None,
                }))
            }
            BankRedirectData::Przelewy24 { .. } => {
                Self::BankRedirect(Box::new(BankRedirectionPMData {
                    payment_brand: PaymentBrand::Przelewy,
                    bank_account_country: None,
                    bank_account_bank_name: None,
                    bank_account_bic: None,
                    bank_account_iban: None,
                    billing_country: None,
                    merchant_customer_id: None,
                    merchant_transaction_id: None,
                    customer_email: Some(item.router_data.get_billing_email()?),
                }))
            }
            BankRedirectData::Interac { .. } => {
                Self::BankRedirect(Box::new(BankRedirectionPMData {
                    payment_brand: PaymentBrand::InteracOnline,
                    bank_account_country: Some(item.router_data.get_billing_country()?),
                    bank_account_bank_name: None,
                    bank_account_bic: None,
                    bank_account_iban: None,
                    billing_country: None,
                    merchant_customer_id: None,
                    merchant_transaction_id: None,
                    customer_email: Some(item.router_data.get_billing_email()?),
                }))
            }
            BankRedirectData::Trustly { .. } => {
                Self::BankRedirect(Box::new(BankRedirectionPMData {
                    payment_brand: PaymentBrand::Trustly,
                    bank_account_country: None,
                    bank_account_bank_name: None,
                    bank_account_bic: None,
                    bank_account_iban: None,
                    billing_country: Some(item.router_data.get_billing_country()?),
                    merchant_customer_id: Some(Secret::new(item.router_data.get_customer_id()?)),
                    merchant_transaction_id: Some(Secret::new(
                        item.router_data.connector_request_reference_id.clone(),
                    )),
                    customer_email: None,
                }))
            }
            BankRedirectData::Bizum { .. }
            | BankRedirectData::Blik { .. }
            | BankRedirectData::BancontactCard { .. }
            | BankRedirectData::OnlineBankingCzechRepublic { .. }
            | BankRedirectData::OnlineBankingFinland { .. }
            | BankRedirectData::OnlineBankingFpx { .. }
            | BankRedirectData::OnlineBankingPoland { .. }
            | BankRedirectData::OnlineBankingSlovakia { .. }
            | BankRedirectData::OnlineBankingThailand { .. }
            | BankRedirectData::LocalBankRedirect {}
            | BankRedirectData::OpenBankingUk { .. } => Err(
                errors::ConnectorError::NotImplemented("Payment method".to_string()),
            )?,
        };
        Ok(payment_data)
    }
}

impl TryFrom<(Card, Option<Secret<String>>)> for PaymentDetails {
    type Error = Error;
    fn try_from(
        (card_data, card_holder_name): (Card, Option<Secret<String>>),
    ) -> Result<Self, Self::Error> {
        Ok(Self::AciCard(Box::new(CardDetails {
            card_number: card_data.card_number,
            card_holder: card_holder_name.unwrap_or(Secret::new("".to_string())),
            card_expiry_month: card_data.card_exp_month,
            card_expiry_year: card_data.card_exp_year,
            card_cvv: card_data.card_cvc,
        })))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankRedirectionPMData {
    payment_brand: PaymentBrand,
    #[serde(rename = "bankAccount.country")]
    bank_account_country: Option<api_models::enums::CountryAlpha2>,
    #[serde(rename = "bankAccount.bankName")]
    bank_account_bank_name: Option<common_enums::BankNames>,
    #[serde(rename = "bankAccount.bic")]
    bank_account_bic: Option<Secret<String>>,
    #[serde(rename = "bankAccount.iban")]
    bank_account_iban: Option<Secret<String>>,
    #[serde(rename = "billing.country")]
    billing_country: Option<api_models::enums::CountryAlpha2>,
    #[serde(rename = "customer.email")]
    customer_email: Option<Email>,
    #[serde(rename = "customer.merchantCustomerId")]
    merchant_customer_id: Option<Secret<id_type::CustomerId>>,
    merchant_transaction_id: Option<Secret<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletPMData {
    payment_brand: PaymentBrand,
    #[serde(rename = "virtualAccount.accountId")]
    account_id: Option<Secret<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentBrand {
    Eps,
    Eft,
    Ideal,
    Giropay,
    Sofortueberweisung,
    InteracOnline,
    Przelewy,
    Trustly,
    Mbway,
    #[serde(rename = "ALIPAY")]
    AliPay,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct CardDetails {
    #[serde(rename = "card.number")]
    pub card_number: cards::CardNumber,
    #[serde(rename = "card.holder")]
    pub card_holder: Secret<String>,
    #[serde(rename = "card.expiryMonth")]
    pub card_expiry_month: Secret<String>,
    #[serde(rename = "card.expiryYear")]
    pub card_expiry_year: Secret<String>,
    #[serde(rename = "card.cvv")]
    pub card_cvv: Secret<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum InstructionMode {
    Initial,
    Repeated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum InstructionType {
    Unscheduled,
}

#[derive(Debug, Clone, Serialize)]
pub enum InstructionSource {
    #[serde(rename = "CIT")]
    CardholderInitiatedTransaction,
    #[serde(rename = "MIT")]
    MerchantInitiatedTransaction,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Instruction {
    #[serde(rename = "standingInstruction.mode")]
    mode: InstructionMode,

    #[serde(rename = "standingInstruction.type")]
    transaction_type: InstructionType,

    #[serde(rename = "standingInstruction.source")]
    source: InstructionSource,

    create_registration: Option<bool>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct BankDetails {
    #[serde(rename = "bankAccount.holder")]
    pub account_holder: Secret<String>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum AciPaymentType {
    #[serde(rename = "PA")]
    Preauthorization,
    #[default]
    #[serde(rename = "DB")]
    Debit,
    #[serde(rename = "CD")]
    Credit,
    #[serde(rename = "CP")]
    Capture,
    #[serde(rename = "RV")]
    Reversal,
    #[serde(rename = "RF")]
    Refund,
}

impl TryFrom<&AciRouterData<&PaymentsAuthorizeRouterData>> for AciPaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &AciRouterData<&PaymentsAuthorizeRouterData>) -> Result<Self, Self::Error> {
        match item.router_data.request.payment_method_data.clone() {
            PaymentMethodData::Card(ref card_data) => Self::try_from((item, card_data)),
            PaymentMethodData::Wallet(ref wallet_data) => Self::try_from((item, wallet_data)),
            PaymentMethodData::PayLater(ref pay_later_data) => {
                Self::try_from((item, pay_later_data))
            }
            PaymentMethodData::BankRedirect(ref bank_redirect_data) => {
                Self::try_from((item, bank_redirect_data))
            }
            PaymentMethodData::MandatePayment => {
                let mandate_id = item.router_data.request.mandate_id.clone().ok_or(
                    errors::ConnectorError::MissingRequiredField {
                        field_name: "mandate_id",
                    },
                )?;
                Self::try_from((item, mandate_id))
            }
            PaymentMethodData::Crypto(_)
            | PaymentMethodData::BankDebit(_)
            | PaymentMethodData::BankTransfer(_)
            | PaymentMethodData::Reward
            | PaymentMethodData::RealTimePayment(_)
            | PaymentMethodData::MobilePayment(_)
            | PaymentMethodData::GiftCard(_)
            | PaymentMethodData::CardRedirect(_)
            | PaymentMethodData::Upi(_)
            | PaymentMethodData::Voucher(_)
            | PaymentMethodData::OpenBanking(_)
            | PaymentMethodData::CardToken(_)
            | PaymentMethodData::NetworkToken(_)
            | PaymentMethodData::CardDetailsForNetworkTransactionId(_) => {
                Err(errors::ConnectorError::NotImplemented(
                    utils::get_unimplemented_payment_method_error_message("Aci"),
                ))?
            }
        }
    }
}

impl TryFrom<(&AciRouterData<&PaymentsAuthorizeRouterData>, &WalletData)> for AciPaymentsRequest {
    type Error = Error;
    fn try_from(
        value: (&AciRouterData<&PaymentsAuthorizeRouterData>, &WalletData),
    ) -> Result<Self, Self::Error> {
        let (item, wallet_data) = value;
        let txn_details = get_transaction_details(item)?;
        let payment_method = PaymentDetails::try_from((wallet_data, item.router_data))?;

        Ok(Self {
            txn_details,
            payment_method,
            instruction: None,
            shopper_result_url: item.router_data.request.router_return_url.clone(),
        })
    }
}

impl
    TryFrom<(
        &AciRouterData<&PaymentsAuthorizeRouterData>,
        &BankRedirectData,
    )> for AciPaymentsRequest
{
    type Error = Error;
    fn try_from(
        value: (
            &AciRouterData<&PaymentsAuthorizeRouterData>,
            &BankRedirectData,
        ),
    ) -> Result<Self, Self::Error> {
        let (item, bank_redirect_data) = value;
        let txn_details = get_transaction_details(item)?;
        let payment_method = PaymentDetails::try_from((item, bank_redirect_data))?;

        Ok(Self {
            txn_details,
            payment_method,
            instruction: None,
            shopper_result_url: item.router_data.request.router_return_url.clone(),
        })
    }
}

impl TryFrom<(&AciRouterData<&PaymentsAuthorizeRouterData>, &PayLaterData)> for AciPaymentsRequest {
    type Error = Error;
    fn try_from(
        value: (&AciRouterData<&PaymentsAuthorizeRouterData>, &PayLaterData),
    ) -> Result<Self, Self::Error> {
        let (item, _pay_later_data) = value;
        let txn_details = get_transaction_details(item)?;
        let payment_method = PaymentDetails::Klarna;

        Ok(Self {
            txn_details,
            payment_method,
            instruction: None,
            shopper_result_url: item.router_data.request.router_return_url.clone(),
        })
    }
}

impl TryFrom<(&AciRouterData<&PaymentsAuthorizeRouterData>, &Card)> for AciPaymentsRequest {
    type Error = Error;
    fn try_from(
        value: (&AciRouterData<&PaymentsAuthorizeRouterData>, &Card),
    ) -> Result<Self, Self::Error> {
        let (item, card_data) = value;
        let card_holder_name = item.router_data.get_optional_billing_full_name();
        let txn_details = get_transaction_details(item)?;
        let payment_method = PaymentDetails::try_from((card_data.clone(), card_holder_name))?;
        let instruction = get_instruction_details(item);

        Ok(Self {
            txn_details,
            payment_method,
            instruction,
            shopper_result_url: None,
        })
    }
}

impl
    TryFrom<(
        &AciRouterData<&PaymentsAuthorizeRouterData>,
        api_models::payments::MandateIds,
    )> for AciPaymentsRequest
{
    type Error = Error;
    fn try_from(
        value: (
            &AciRouterData<&PaymentsAuthorizeRouterData>,
            api_models::payments::MandateIds,
        ),
    ) -> Result<Self, Self::Error> {
        let (item, _mandate_data) = value;
        let instruction = get_instruction_details(item);
        let txn_details = get_transaction_details(item)?;

        Ok(Self {
            txn_details,
            payment_method: PaymentDetails::Mandate,
            instruction,
            shopper_result_url: item.router_data.request.router_return_url.clone(),
        })
    }
}

fn get_transaction_details(
    item: &AciRouterData<&PaymentsAuthorizeRouterData>,
) -> Result<TransactionDetails, error_stack::Report<errors::ConnectorError>> {
    let auth = AciAuthType::try_from(&item.router_data.connector_auth_type)?;
    Ok(TransactionDetails {
        entity_id: auth.entity_id,
        amount: item.amount.to_owned(),
        currency: item.router_data.request.currency.to_string(),
        payment_type: AciPaymentType::Debit,
    })
}

fn get_instruction_details(
    item: &AciRouterData<&PaymentsAuthorizeRouterData>,
) -> Option<Instruction> {
    if item.router_data.request.setup_mandate_details.is_some() {
        return Some(Instruction {
            mode: InstructionMode::Initial,
            transaction_type: InstructionType::Unscheduled,
            source: InstructionSource::CardholderInitiatedTransaction,
            create_registration: Some(true),
        });
    } else if item.router_data.request.mandate_id.is_some() {
        return Some(Instruction {
            mode: InstructionMode::Repeated,
            transaction_type: InstructionType::Unscheduled,
            source: InstructionSource::MerchantInitiatedTransaction,
            create_registration: None,
        });
    }
    None
}

impl TryFrom<&PaymentsCancelRouterData> for AciCancelRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &PaymentsCancelRouterData) -> Result<Self, Self::Error> {
        let auth = AciAuthType::try_from(&item.connector_auth_type)?;
        let aci_payment_request = Self {
            entity_id: auth.entity_id,
            payment_type: AciPaymentType::Reversal,
        };
        Ok(aci_payment_request)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AciPaymentStatus {
    Succeeded,
    Failed,
    #[default]
    Pending,
    RedirectShopper,
}

fn map_aci_attempt_status(item: AciPaymentStatus, auto_capture: bool) -> enums::AttemptStatus {
    match item {
        AciPaymentStatus::Succeeded => {
            if auto_capture {
                enums::AttemptStatus::Charged
            } else {
                enums::AttemptStatus::Authorized
            }
        }
        AciPaymentStatus::Failed => enums::AttemptStatus::Failure,
        AciPaymentStatus::Pending => enums::AttemptStatus::Authorizing,
        AciPaymentStatus::RedirectShopper => enums::AttemptStatus::AuthenticationPending,
    }
}
impl FromStr for AciPaymentStatus {
    type Err = error_stack::Report<errors::ConnectorError>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if FAILURE_CODES.contains(&s) {
            Ok(Self::Failed)
        } else if PENDING_CODES.contains(&s) {
            Ok(Self::Pending)
        } else if SUCCESSFUL_CODES.contains(&s) {
            Ok(Self::Succeeded)
        } else {
            Err(report!(errors::ConnectorError::UnexpectedResponseError(
                bytes::Bytes::from(s.to_owned())
            )))
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciPaymentsResponse {
    id: String,
    registration_id: Option<Secret<String>>,
    // ndc is an internal unique identifier for the request.
    ndc: String,
    timestamp: String,
    // Number useful for support purposes.
    build_number: String,
    pub(super) result: ResultCode,
    pub(super) redirect: Option<AciRedirectionData>,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciErrorResponse {
    ndc: String,
    timestamp: String,
    build_number: String,
    pub(super) result: ResultCode,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciRedirectionData {
    method: Option<Method>,
    parameters: Vec<Parameters>,
    url: Url,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Parameters {
    name: String,
    value: String,
}

#[derive(Default, Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultCode {
    pub(super) code: String,
    pub(super) description: String,
    pub(super) parameter_errors: Option<Vec<ErrorParameters>>,
}

#[derive(Default, Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct ErrorParameters {
    pub(super) name: String,
    pub(super) value: Option<String>,
    pub(super) message: String,
}

impl<F, Req> TryFrom<ResponseRouterData<F, AciPaymentsResponse, Req, PaymentsResponseData>>
    for RouterData<F, Req, PaymentsResponseData>
where
    Req: GetCaptureMethod,
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, AciPaymentsResponse, Req, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let redirection_data = item.response.redirect.map(|data| {
            let form_fields = std::collections::HashMap::<_, _>::from_iter(
                data.parameters
                    .iter()
                    .map(|parameter| (parameter.name.clone(), parameter.value.clone())),
            );

            // If method is Get, parameters are appended to URL
            // If method is post, we http Post the method to URL
            RedirectForm::Form {
                endpoint: data.url.to_string(),
                // Handles method for Bank redirects currently.
                // 3DS response have method within preconditions. That would require replacing below line with a function.
                method: data.method.unwrap_or(Method::Post),
                form_fields,
            }
        });

        let mandate_reference = item.response.registration_id.map(|id| MandateReference {
            connector_mandate_id: Some(id.expose()),
            payment_method_id: None,
            mandate_metadata: None,
            connector_mandate_request_reference_id: None,
        });

        let auto_capture = matches!(
            item.data.request.get_capture_method(),
            Some(enums::CaptureMethod::Automatic) | None
        );

        let status = if redirection_data.is_some() {
            map_aci_attempt_status(AciPaymentStatus::RedirectShopper, auto_capture)
        } else {
            map_aci_attempt_status(
                AciPaymentStatus::from_str(&item.response.result.code)?,
                auto_capture,
            )
        };

        Ok(Self {
            status,
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.id.clone()),
                redirection_data: Box::new(redirection_data),
                mandate_reference: Box::new(mandate_reference),
                connector_metadata: None,
                network_txn_id: None,
                connector_response_reference_id: Some(item.response.id),
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciCaptureRequest {
    #[serde(flatten)]
    pub txn_details: TransactionDetails,
}

impl TryFrom<&AciRouterData<&PaymentsCaptureRouterData>> for AciCaptureRequest {
    type Error = error_stack::Report<errors::ConnectorError>;

    fn try_from(item: &AciRouterData<&PaymentsCaptureRouterData>) -> Result<Self, Self::Error> {
        let auth = AciAuthType::try_from(&item.router_data.connector_auth_type)?;
        Ok(Self {
            txn_details: TransactionDetails {
                entity_id: auth.entity_id,
                amount: item.amount.to_owned(),
                currency: item.router_data.request.currency.to_string(),
                payment_type: AciPaymentType::Capture,
            },
        })
    }
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciCaptureResponse {
    id: String,
    referenced_id: String,
    payment_type: AciPaymentType,
    amount: StringMajorUnit,
    currency: String,
    descriptor: String,
    result: AciCaptureResult,
    result_details: AciCaptureResultDetails,
    build_number: String,
    timestamp: String,
    ndc: Secret<String>,
    source: Secret<String>,
    payment_method: String,
    short_id: String,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciCaptureResult {
    code: String,
    description: String,
}

#[derive(Debug, Default, Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AciCaptureResultDetails {
    extended_description: String,
    #[serde(rename = "clearingInstituteName")]
    clearing_institute_name: String,
    connector_tx_id1: String,
    connector_tx_id3: String,
    connector_tx_id2: String,
    acquirer_response: String,
}

impl<F, T> TryFrom<ResponseRouterData<F, AciCaptureResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, AciCaptureResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            status: map_aci_attempt_status(
                AciPaymentStatus::from_str(&item.response.result.code)?,
                false,
            ),
            reference_id: Some(item.response.referenced_id.clone()),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id: ResponseId::ConnectorTransactionId(item.response.id.clone()),
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: None,
                network_txn_id: None,
                connector_response_reference_id: Some(item.response.referenced_id),
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciRefundRequest {
    pub amount: StringMajorUnit,
    pub currency: String,
    pub payment_type: AciPaymentType,
    pub entity_id: Secret<String>,
}

impl<F> TryFrom<&AciRouterData<&RefundsRouterData<F>>> for AciRefundRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &AciRouterData<&RefundsRouterData<F>>) -> Result<Self, Self::Error> {
        let amount = item.amount.to_owned();
        let currency = item.router_data.request.currency;
        let payment_type = AciPaymentType::Refund;
        let auth = AciAuthType::try_from(&item.router_data.connector_auth_type)?;

        Ok(Self {
            amount,
            currency: currency.to_string(),
            payment_type,
            entity_id: auth.entity_id,
        })
    }
}

#[derive(Debug, Default, Deserialize, Clone)]
pub enum AciRefundStatus {
    Succeeded,
    Failed,
    #[default]
    Pending,
}

impl FromStr for AciRefundStatus {
    type Err = error_stack::Report<errors::ConnectorError>;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if FAILURE_CODES.contains(&s) {
            Ok(Self::Failed)
        } else if PENDING_CODES.contains(&s) {
            Ok(Self::Pending)
        } else if SUCCESSFUL_CODES.contains(&s) {
            Ok(Self::Succeeded)
        } else {
            Err(report!(errors::ConnectorError::UnexpectedResponseError(
                bytes::Bytes::from(s.to_owned())
            )))
        }
    }
}

impl From<AciRefundStatus> for enums::RefundStatus {
    fn from(item: AciRefundStatus) -> Self {
        match item {
            AciRefundStatus::Succeeded => Self::Success,
            AciRefundStatus::Failed => Self::Failure,
            AciRefundStatus::Pending => Self::Pending,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciRefundResponse {
    id: String,
    //ndc is an internal unique identifier for the request.
    ndc: String,
    timestamp: String,
    build_number: String,
    pub(super) result: ResultCode,
}

impl<F> TryFrom<RefundsResponseRouterData<F, AciRefundResponse>> for RefundsRouterData<F> {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: RefundsResponseRouterData<F, AciRefundResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(RefundsResponseData {
                connector_refund_id: item.response.id,
                refund_status: enums::RefundStatus::from(AciRefundStatus::from_str(
                    &item.response.result.code,
                )?),
            }),
            ..item.data
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum AciWebhookEventType {
    Payment,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum AciWebhookAction {
    Created,
    Updated,
    Deleted,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciWebhookCardDetails {
    pub bin: Option<String>,
    #[serde(rename = "last4Digits")]
    pub last4_digits: Option<String>,
    pub holder: Option<String>,
    pub expiry_month: Option<Secret<String>>,
    pub expiry_year: Option<Secret<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciWebhookCustomerDetails {
    #[serde(rename = "givenName")]
    pub given_name: Option<Secret<String>>,
    pub surname: Option<Secret<String>>,
    #[serde(rename = "merchantCustomerId")]
    pub merchant_customer_id: Option<Secret<String>>,
    pub sex: Option<Secret<String>>,
    pub email: Option<Email>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciWebhookAuthenticationDetails {
    #[serde(rename = "entityId")]
    pub entity_id: Secret<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciWebhookRiskDetails {
    pub score: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciPaymentWebhookPayload {
    pub id: String,
    pub payment_type: String,
    pub payment_brand: String,
    pub amount: StringMajorUnit,
    pub currency: String,
    pub presentation_amount: Option<StringMajorUnit>,
    pub presentation_currency: Option<String>,
    pub descriptor: Option<String>,
    pub result: ResultCode,
    pub authentication: Option<AciWebhookAuthenticationDetails>,
    pub card: Option<AciWebhookCardDetails>,
    pub customer: Option<AciWebhookCustomerDetails>,
    #[serde(rename = "customParameters")]
    pub custom_parameters: Option<serde_json::Value>,
    pub risk: Option<AciWebhookRiskDetails>,
    pub build_number: Option<String>,
    pub timestamp: String,
    pub ndc: String,
    #[serde(rename = "channelName")]
    pub channel_name: Option<String>,
    pub source: Option<String>,
    pub payment_method: Option<String>,
    #[serde(rename = "shortId")]
    pub short_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AciWebhookNotification {
    #[serde(rename = "type")]
    pub event_type: AciWebhookEventType,
    pub action: Option<AciWebhookAction>,
    pub payload: serde_json::Value,
}
