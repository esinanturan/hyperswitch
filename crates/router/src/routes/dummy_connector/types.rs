use api_models::enums::Currency;
use common_utils::{errors::CustomResult, generate_id_with_default_len, pii};
use error_stack::report;
use masking::Secret;
use router_env::types::FlowMetric;
use strum::Display;
use time::PrimitiveDateTime;

use super::{consts, errors::DummyConnectorErrors};
use crate::services;

#[derive(Debug, Display, Clone, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Flow {
    DummyPaymentCreate,
    DummyPaymentRetrieve,
    DummyPaymentAuthorize,
    DummyPaymentComplete,
    DummyRefundCreate,
    DummyRefundRetrieve,
}

impl FlowMetric for Flow {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, strum::Display, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum DummyConnectors {
    #[serde(rename = "phonypay")]
    #[strum(serialize = "phonypay")]
    PhonyPay,
    #[serde(rename = "fauxpay")]
    #[strum(serialize = "fauxpay")]
    FauxPay,
    #[serde(rename = "pretendpay")]
    #[strum(serialize = "pretendpay")]
    PretendPay,
    StripeTest,
    AdyenTest,
    CheckoutTest,
    PaypalTest,
}

impl DummyConnectors {
    pub fn get_connector_image_link(self, base_url: &str) -> String {
        let image_name = match self {
            Self::PhonyPay => "PHONYPAY.svg",
            Self::FauxPay => "FAUXPAY.svg",
            Self::PretendPay => "PRETENDPAY.svg",
            Self::StripeTest => "STRIPE_TEST.svg",
            Self::PaypalTest => "PAYPAL_TEST.svg",
            _ => "PHONYPAY.svg",
        };
        format!("{base_url}{image_name}")
    }
}

#[derive(
    Default, serde::Serialize, serde::Deserialize, strum::Display, Clone, PartialEq, Debug, Eq,
)]
#[serde(rename_all = "lowercase")]
pub enum DummyConnectorStatus {
    Succeeded,
    #[default]
    Processing,
    Failed,
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
pub struct DummyConnectorPaymentAttempt {
    pub timestamp: PrimitiveDateTime,
    pub attempt_id: String,
    pub payment_id: common_utils::id_type::PaymentId,
    pub payment_request: DummyConnectorPaymentRequest,
}

impl From<DummyConnectorPaymentRequest> for DummyConnectorPaymentAttempt {
    fn from(payment_request: DummyConnectorPaymentRequest) -> Self {
        let timestamp = common_utils::date_time::now();
        let payment_id = common_utils::id_type::PaymentId::default();
        let attempt_id = generate_id_with_default_len(consts::ATTEMPT_ID_PREFIX);
        Self {
            timestamp,
            attempt_id,
            payment_id,
            payment_request,
        }
    }
}

impl DummyConnectorPaymentAttempt {
    pub fn build_payment_data(
        self,
        status: DummyConnectorStatus,
        next_action: Option<DummyConnectorNextAction>,
        return_url: Option<String>,
    ) -> DummyConnectorPaymentData {
        DummyConnectorPaymentData {
            attempt_id: self.attempt_id,
            payment_id: self.payment_id,
            status,
            amount: self.payment_request.amount,
            eligible_amount: self.payment_request.amount,
            connector: self.payment_request.connector,
            created: self.timestamp,
            currency: self.payment_request.currency,
            payment_method_type: self.payment_request.payment_method_data.into(),
            next_action,
            return_url,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
pub struct DummyConnectorPaymentRequest {
    pub amount: i64,
    pub currency: Currency,
    pub payment_method_data: DummyConnectorPaymentMethodData,
    pub return_url: Option<String>,
    pub connector: DummyConnectors,
}

pub trait GetPaymentMethodDetails {
    fn get_name(&self) -> &'static str;
    fn get_image_link(&self, base_url: &str) -> String;
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DummyConnectorPaymentMethodData {
    Card(DummyConnectorCard),
    Upi(DummyConnectorUpi),
    Wallet(DummyConnectorWallet),
    PayLater(DummyConnectorPayLater),
}

#[derive(
    Default, serde::Serialize, serde::Deserialize, strum::Display, PartialEq, Debug, Clone,
)]
#[serde(rename_all = "lowercase")]
pub enum DummyConnectorPaymentMethodType {
    #[default]
    Card,
    Upi(DummyConnectorUpiType),
    Wallet(DummyConnectorWallet),
    PayLater(DummyConnectorPayLater),
}

impl From<DummyConnectorPaymentMethodData> for DummyConnectorPaymentMethodType {
    fn from(value: DummyConnectorPaymentMethodData) -> Self {
        match value {
            DummyConnectorPaymentMethodData::Card(_) => Self::Card,
            DummyConnectorPaymentMethodData::Upi(upi_data) => match upi_data {
                DummyConnectorUpi::UpiCollect(_) => Self::Upi(DummyConnectorUpiType::UpiCollect),
            },
            DummyConnectorPaymentMethodData::Wallet(wallet) => Self::Wallet(wallet),
            DummyConnectorPaymentMethodData::PayLater(pay_later) => Self::PayLater(pay_later),
        }
    }
}

impl GetPaymentMethodDetails for DummyConnectorPaymentMethodType {
    fn get_name(&self) -> &'static str {
        match self {
            Self::Card => "3D Secure",
            Self::Upi(upi_type) => upi_type.get_name(),
            Self::Wallet(wallet) => wallet.get_name(),
            Self::PayLater(pay_later) => pay_later.get_name(),
        }
    }

    fn get_image_link(&self, base_url: &str) -> String {
        match self {
            Self::Card => format!("{}{}", base_url, "CARD.svg"),
            Self::Upi(upi_type) => upi_type.get_image_link(base_url),
            Self::Wallet(wallet) => wallet.get_image_link(base_url),
            Self::PayLater(pay_later) => pay_later.get_image_link(base_url),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorCard {
    pub name: Secret<String>,
    pub number: cards::CardNumber,
    pub expiry_month: Secret<String>,
    pub expiry_year: Secret<String>,
    pub cvc: Secret<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorUpiCollect {
    pub vpa_id: Secret<String, pii::UpiVpaMaskingStrategy>,
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DummyConnectorUpi {
    UpiCollect(DummyConnectorUpiCollect),
}

pub enum DummyConnectorCardFlow {
    NoThreeDS(DummyConnectorStatus, Option<DummyConnectorErrors>),
    ThreeDS(DummyConnectorStatus, Option<DummyConnectorErrors>),
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
pub enum DummyConnectorWallet {
    GooglePay,
    Paypal,
    WeChatPay,
    MbWay,
    AliPay,
    AliPayHK,
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
pub enum DummyConnectorUpiType {
    UpiCollect,
}

impl GetPaymentMethodDetails for DummyConnectorUpiType {
    fn get_name(&self) -> &'static str {
        match self {
            Self::UpiCollect => "UPI Collect",
        }
    }
    fn get_image_link(&self, base_url: &str) -> String {
        let image_name = match self {
            Self::UpiCollect => "UPI_COLLECT.svg",
        };
        format!("{base_url}{image_name}")
    }
}

impl GetPaymentMethodDetails for DummyConnectorWallet {
    fn get_name(&self) -> &'static str {
        match self {
            Self::GooglePay => "Google Pay",
            Self::Paypal => "PayPal",
            Self::WeChatPay => "WeChat Pay",
            Self::MbWay => "Mb Way",
            Self::AliPay => "Alipay",
            Self::AliPayHK => "Alipay HK",
        }
    }
    fn get_image_link(&self, base_url: &str) -> String {
        let image_name = match self {
            Self::GooglePay => "GOOGLE_PAY.svg",
            Self::Paypal => "PAYPAL.svg",
            Self::WeChatPay => "WECHAT_PAY.svg",
            Self::MbWay => "MBWAY.svg",
            Self::AliPay => "ALIPAY.svg",
            Self::AliPayHK => "ALIPAY.svg",
        };
        format!("{base_url}{image_name}")
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Eq, PartialEq)]
pub enum DummyConnectorPayLater {
    Klarna,
    Affirm,
    AfterPayClearPay,
}

impl GetPaymentMethodDetails for DummyConnectorPayLater {
    fn get_name(&self) -> &'static str {
        match self {
            Self::Klarna => "Klarna",
            Self::Affirm => "Affirm",
            Self::AfterPayClearPay => "Afterpay Clearpay",
        }
    }
    fn get_image_link(&self, base_url: &str) -> String {
        let image_name = match self {
            Self::Klarna => "KLARNA.svg",
            Self::Affirm => "AFFIRM.svg",
            Self::AfterPayClearPay => "AFTERPAY.svg",
        };
        format!("{base_url}{image_name}")
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DummyConnectorPaymentData {
    pub attempt_id: String,
    pub payment_id: common_utils::id_type::PaymentId,
    pub status: DummyConnectorStatus,
    pub amount: i64,
    pub eligible_amount: i64,
    pub currency: Currency,
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub created: PrimitiveDateTime,
    pub payment_method_type: DummyConnectorPaymentMethodType,
    pub connector: DummyConnectors,
    pub next_action: Option<DummyConnectorNextAction>,
    pub return_url: Option<String>,
}

impl DummyConnectorPaymentData {
    pub fn is_eligible_for_refund(&self, refund_amount: i64) -> DummyConnectorResult<()> {
        if self.eligible_amount < refund_amount {
            return Err(
                report!(DummyConnectorErrors::RefundAmountExceedsPaymentAmount)
                    .attach_printable("Eligible amount is lesser than refund amount"),
            );
        }
        if self.status != DummyConnectorStatus::Succeeded {
            return Err(report!(DummyConnectorErrors::PaymentNotSuccessful)
                .attach_printable("Payment is not successful to process the refund"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DummyConnectorNextAction {
    RedirectToUrl(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorPaymentResponse {
    pub status: DummyConnectorStatus,
    pub id: common_utils::id_type::PaymentId,
    pub amount: i64,
    pub currency: Currency,
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub created: PrimitiveDateTime,
    pub payment_method_type: DummyConnectorPaymentMethodType,
    pub next_action: Option<DummyConnectorNextAction>,
}

impl From<DummyConnectorPaymentData> for DummyConnectorPaymentResponse {
    fn from(value: DummyConnectorPaymentData) -> Self {
        Self {
            status: value.status,
            id: value.payment_id,
            amount: value.amount,
            currency: value.currency,
            created: value.created,
            payment_method_type: value.payment_method_type,
            next_action: value.next_action,
        }
    }
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorPaymentRetrieveRequest {
    pub payment_id: String,
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorPaymentConfirmRequest {
    pub attempt_id: String,
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorPaymentCompleteRequest {
    pub attempt_id: String,
    pub confirm: bool,
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorPaymentCompleteBody {
    pub confirm: bool,
}

#[derive(Default, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
pub struct DummyConnectorRefundRequest {
    pub amount: i64,
    pub payment_id: Option<common_utils::id_type::PaymentId>,
}

#[derive(Clone, Debug, serde::Serialize, Eq, PartialEq, serde::Deserialize)]
pub struct DummyConnectorRefundResponse {
    pub status: DummyConnectorStatus,
    pub id: String,
    pub currency: Currency,
    #[serde(with = "common_utils::custom_serde::iso8601")]
    pub created: PrimitiveDateTime,
    pub payment_amount: i64,
    pub refund_amount: i64,
}

impl DummyConnectorRefundResponse {
    pub fn new(
        status: DummyConnectorStatus,
        id: String,
        currency: Currency,
        created: PrimitiveDateTime,
        payment_amount: i64,
        refund_amount: i64,
    ) -> Self {
        Self {
            status,
            id,
            currency,
            created,
            payment_amount,
            refund_amount,
        }
    }
}

#[derive(Default, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DummyConnectorRefundRetrieveRequest {
    pub refund_id: String,
}

pub type DummyConnectorResponse<T> =
    CustomResult<services::ApplicationResponse<T>, DummyConnectorErrors>;

pub type DummyConnectorResult<T> = CustomResult<T, DummyConnectorErrors>;

pub struct DummyConnectorUpiFlow {
    pub status: DummyConnectorStatus,
    pub error: Option<DummyConnectorErrors>,
    pub is_next_action_required: bool,
}
