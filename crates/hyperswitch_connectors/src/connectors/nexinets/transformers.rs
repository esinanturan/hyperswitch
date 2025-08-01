use base64::Engine;
use cards::CardNumber;
use common_enums::{enums, AttemptStatus};
use common_utils::{consts, errors::CustomResult, request::Method};
use error_stack::ResultExt;
use hyperswitch_domain_models::{
    payment_method_data::{
        ApplePayWalletData, BankRedirectData, Card, PaymentMethodData, WalletData,
    },
    router_data::{ConnectorAuthType, RouterData},
    router_flow_types::{Execute, RSync},
    router_request_types::ResponseId,
    router_response_types::{
        MandateReference, PaymentsResponseData, RedirectForm, RefundsResponseData,
    },
    types::{
        PaymentsAuthorizeRouterData, PaymentsCancelRouterData, PaymentsCaptureRouterData,
        RefundsRouterData,
    },
};
use hyperswitch_interfaces::errors;
use masking::{ExposeInterface, PeekInterface, Secret};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    types::{RefundsResponseRouterData, ResponseRouterData},
    utils::{
        self, CardData, PaymentsAuthorizeRequestData, PaymentsCancelRequestData, WalletData as _,
    },
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsPaymentsRequest {
    initial_amount: i64,
    currency: enums::Currency,
    channel: NexinetsChannel,
    product: NexinetsProduct,
    payment: Option<NexinetsPaymentDetails>,
    #[serde(rename = "async")]
    nexinets_async: NexinetsAsyncDetails,
    merchant_order_id: Option<String>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NexinetsChannel {
    #[default]
    Ecom,
}

#[derive(Default, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NexinetsProduct {
    #[default]
    Creditcard,
    Paypal,
    Giropay,
    Sofort,
    Eps,
    Ideal,
    Applepay,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum NexinetsPaymentDetails {
    Card(Box<NexiCardDetails>),
    Wallet(Box<NexinetsWalletDetails>),
    BankRedirects(Box<NexinetsBankRedirects>),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexiCardDetails {
    #[serde(flatten)]
    card_data: CardDataDetails,
    cof_contract: Option<CofContract>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum CardDataDetails {
    CardDetails(Box<CardDetails>),
    PaymentInstrument(Box<PaymentInstrument>),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDetails {
    card_number: CardNumber,
    expiry_month: Secret<String>,
    expiry_year: Secret<String>,
    verification: Secret<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentInstrument {
    payment_instrument_id: Option<Secret<String>>,
}

#[derive(Debug, Serialize)]
pub struct CofContract {
    #[serde(rename = "type")]
    recurring_type: RecurringType,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecurringType {
    Unscheduled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsBankRedirects {
    bic: Option<NexinetsBIC>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsAsyncDetails {
    pub success_url: Option<String>,
    pub cancel_url: Option<String>,
    pub failure_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub enum NexinetsBIC {
    #[serde(rename = "ABNANL2A")]
    AbnAmro,
    #[serde(rename = "ASNBNL21")]
    AsnBank,
    #[serde(rename = "BUNQNL2A")]
    Bunq,
    #[serde(rename = "INGBNL2A")]
    Ing,
    #[serde(rename = "KNABNL2H")]
    Knab,
    #[serde(rename = "RABONL2U")]
    Rabobank,
    #[serde(rename = "RBRBNL21")]
    Regiobank,
    #[serde(rename = "SNSBNL2A")]
    SnsBank,
    #[serde(rename = "TRIONL2U")]
    TriodosBank,
    #[serde(rename = "FVLBNL22")]
    VanLanschot,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NexinetsWalletDetails {
    ApplePayToken(Box<ApplePayDetails>),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplePayDetails {
    payment_data: serde_json::Value,
    payment_method: ApplepayPaymentMethod,
    transaction_identifier: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplepayPaymentMethod {
    display_name: String,
    network: String,
    #[serde(rename = "type")]
    token_type: String,
}

impl TryFrom<&PaymentsAuthorizeRouterData> for NexinetsPaymentsRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &PaymentsAuthorizeRouterData) -> Result<Self, Self::Error> {
        let return_url = item.request.router_return_url.clone();
        let nexinets_async = NexinetsAsyncDetails {
            success_url: return_url.clone(),
            cancel_url: return_url.clone(),
            failure_url: return_url,
        };
        let (payment, product) = get_payment_details_and_product(item)?;
        let merchant_order_id = match item.payment_method {
            // Merchant order id is sent only in case of card payment
            enums::PaymentMethod::Card => Some(item.connector_request_reference_id.clone()),
            _ => None,
        };
        Ok(Self {
            initial_amount: item.request.amount,
            currency: item.request.currency,
            channel: NexinetsChannel::Ecom,
            product,
            payment,
            nexinets_async,
            merchant_order_id,
        })
    }
}

// Auth Struct
pub struct NexinetsAuthType {
    pub(super) api_key: Secret<String>,
}

impl TryFrom<&ConnectorAuthType> for NexinetsAuthType {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(auth_type: &ConnectorAuthType) -> Result<Self, Self::Error> {
        match auth_type {
            ConnectorAuthType::BodyKey { api_key, key1 } => {
                let auth_key = format!("{}:{}", key1.peek(), api_key.peek());
                let auth_header = format!("Basic {}", consts::BASE64_ENGINE.encode(auth_key));
                Ok(Self {
                    api_key: Secret::new(auth_header),
                })
            }
            _ => Err(errors::ConnectorError::FailedToObtainAuthType)?,
        }
    }
}
// PaymentsResponse
#[derive(Debug, Deserialize, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NexinetsPaymentStatus {
    Success,
    Pending,
    Ok,
    Failure,
    Declined,
    InProgress,
    Expired,
    Aborted,
}

fn get_status(status: NexinetsPaymentStatus, method: NexinetsTransactionType) -> AttemptStatus {
    match status {
        NexinetsPaymentStatus::Success => match method {
            NexinetsTransactionType::Preauth => AttemptStatus::Authorized,
            NexinetsTransactionType::Debit | NexinetsTransactionType::Capture => {
                AttemptStatus::Charged
            }
            NexinetsTransactionType::Cancel => AttemptStatus::Voided,
        },
        NexinetsPaymentStatus::Declined
        | NexinetsPaymentStatus::Failure
        | NexinetsPaymentStatus::Expired
        | NexinetsPaymentStatus::Aborted => match method {
            NexinetsTransactionType::Preauth => AttemptStatus::AuthorizationFailed,
            NexinetsTransactionType::Debit | NexinetsTransactionType::Capture => {
                AttemptStatus::CaptureFailed
            }
            NexinetsTransactionType::Cancel => AttemptStatus::VoidFailed,
        },
        NexinetsPaymentStatus::Ok => match method {
            NexinetsTransactionType::Preauth => AttemptStatus::Authorized,
            _ => AttemptStatus::Pending,
        },
        NexinetsPaymentStatus::Pending => AttemptStatus::AuthenticationPending,
        NexinetsPaymentStatus::InProgress => AttemptStatus::Pending,
    }
}

impl TryFrom<&enums::BankNames> for NexinetsBIC {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(bank: &enums::BankNames) -> Result<Self, Self::Error> {
        match bank {
            enums::BankNames::AbnAmro => Ok(Self::AbnAmro),
            enums::BankNames::AsnBank => Ok(Self::AsnBank),
            enums::BankNames::Bunq => Ok(Self::Bunq),
            enums::BankNames::Ing => Ok(Self::Ing),
            enums::BankNames::Knab => Ok(Self::Knab),
            enums::BankNames::Rabobank => Ok(Self::Rabobank),
            enums::BankNames::Regiobank => Ok(Self::Regiobank),
            enums::BankNames::SnsBank => Ok(Self::SnsBank),
            enums::BankNames::TriodosBank => Ok(Self::TriodosBank),
            enums::BankNames::VanLanschot => Ok(Self::VanLanschot),
            _ => Err(errors::ConnectorError::FlowNotSupported {
                flow: bank.to_string(),
                connector: "Nexinets".to_string(),
            }
            .into()),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsPreAuthOrDebitResponse {
    order_id: String,
    transaction_type: NexinetsTransactionType,
    transactions: Vec<NexinetsTransaction>,
    payment_instrument: PaymentInstrument,
    redirect_url: Option<Url>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsTransaction {
    pub transaction_id: String,
    #[serde(rename = "type")]
    pub transaction_type: NexinetsTransactionType,
    pub currency: enums::Currency,
    pub status: NexinetsPaymentStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NexinetsTransactionType {
    Preauth,
    Debit,
    Capture,
    Cancel,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NexinetsPaymentsMetadata {
    pub transaction_id: Option<String>,
    pub order_id: Option<String>,
    pub psync_flow: NexinetsTransactionType,
}

impl<F, T> TryFrom<ResponseRouterData<F, NexinetsPreAuthOrDebitResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, NexinetsPreAuthOrDebitResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let transaction = match item.response.transactions.first() {
            Some(order) => order,
            _ => Err(errors::ConnectorError::ResponseHandlingFailed)?,
        };
        let connector_metadata = serde_json::to_value(NexinetsPaymentsMetadata {
            transaction_id: Some(transaction.transaction_id.clone()),
            order_id: Some(item.response.order_id.clone()),
            psync_flow: item.response.transaction_type.clone(),
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)?;
        let redirection_data = item
            .response
            .redirect_url
            .map(|url| RedirectForm::from((url, Method::Get)));
        let resource_id = match item.response.transaction_type.clone() {
            NexinetsTransactionType::Preauth => ResponseId::NoResponseId,
            NexinetsTransactionType::Debit => {
                ResponseId::ConnectorTransactionId(transaction.transaction_id.clone())
            }
            _ => Err(errors::ConnectorError::ResponseHandlingFailed)?,
        };
        let mandate_reference = item
            .response
            .payment_instrument
            .payment_instrument_id
            .map(|id| MandateReference {
                connector_mandate_id: Some(id.expose()),
                payment_method_id: None,
                mandate_metadata: None,
                connector_mandate_request_reference_id: None,
            });
        Ok(Self {
            status: get_status(transaction.status.clone(), item.response.transaction_type),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id,
                redirection_data: Box::new(redirection_data),
                mandate_reference: Box::new(mandate_reference),
                connector_metadata: Some(connector_metadata),
                network_txn_id: None,
                connector_response_reference_id: Some(item.response.order_id),
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsCaptureOrVoidRequest {
    pub initial_amount: i64,
    pub currency: enums::Currency,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsOrder {
    pub order_id: String,
}

impl TryFrom<&PaymentsCaptureRouterData> for NexinetsCaptureOrVoidRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &PaymentsCaptureRouterData) -> Result<Self, Self::Error> {
        Ok(Self {
            initial_amount: item.request.amount_to_capture,
            currency: item.request.currency,
        })
    }
}

impl TryFrom<&PaymentsCancelRouterData> for NexinetsCaptureOrVoidRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &PaymentsCancelRouterData) -> Result<Self, Self::Error> {
        Ok(Self {
            initial_amount: item.request.get_amount()?,
            currency: item.request.get_currency()?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsPaymentResponse {
    pub transaction_id: String,
    pub status: NexinetsPaymentStatus,
    pub order: NexinetsOrder,
    #[serde(rename = "type")]
    pub transaction_type: NexinetsTransactionType,
}

impl<F, T> TryFrom<ResponseRouterData<F, NexinetsPaymentResponse, T, PaymentsResponseData>>
    for RouterData<F, T, PaymentsResponseData>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: ResponseRouterData<F, NexinetsPaymentResponse, T, PaymentsResponseData>,
    ) -> Result<Self, Self::Error> {
        let transaction_id = Some(item.response.transaction_id.clone());
        let connector_metadata = serde_json::to_value(NexinetsPaymentsMetadata {
            transaction_id,
            order_id: Some(item.response.order.order_id.clone()),
            psync_flow: item.response.transaction_type.clone(),
        })
        .change_context(errors::ConnectorError::ResponseHandlingFailed)?;
        let resource_id = match item.response.transaction_type.clone() {
            NexinetsTransactionType::Debit | NexinetsTransactionType::Capture => {
                ResponseId::ConnectorTransactionId(item.response.transaction_id)
            }
            _ => ResponseId::NoResponseId,
        };
        Ok(Self {
            status: get_status(item.response.status, item.response.transaction_type),
            response: Ok(PaymentsResponseData::TransactionResponse {
                resource_id,
                redirection_data: Box::new(None),
                mandate_reference: Box::new(None),
                connector_metadata: Some(connector_metadata),
                network_txn_id: None,
                connector_response_reference_id: Some(item.response.order.order_id),
                incremental_authorization_allowed: None,
                charges: None,
            }),
            ..item.data
        })
    }
}

// REFUND :
// Type definition for RefundRequest
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsRefundRequest {
    pub initial_amount: i64,
    pub currency: enums::Currency,
}

impl<F> TryFrom<&RefundsRouterData<F>> for NexinetsRefundRequest {
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(item: &RefundsRouterData<F>) -> Result<Self, Self::Error> {
        Ok(Self {
            initial_amount: item.request.refund_amount,
            currency: item.request.currency,
        })
    }
}

// Type definition for Refund Response
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NexinetsRefundResponse {
    pub transaction_id: String,
    pub status: RefundStatus,
    pub order: NexinetsOrder,
    #[serde(rename = "type")]
    pub transaction_type: RefundType,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundStatus {
    Success,
    Ok,
    Failure,
    Declined,
    InProgress,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundType {
    Refund,
}

impl From<RefundStatus> for enums::RefundStatus {
    fn from(item: RefundStatus) -> Self {
        match item {
            RefundStatus::Success => Self::Success,
            RefundStatus::Failure | RefundStatus::Declined => Self::Failure,
            RefundStatus::InProgress | RefundStatus::Ok => Self::Pending,
        }
    }
}

impl TryFrom<RefundsResponseRouterData<Execute, NexinetsRefundResponse>>
    for RefundsRouterData<Execute>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: RefundsResponseRouterData<Execute, NexinetsRefundResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(RefundsResponseData {
                connector_refund_id: item.response.transaction_id,
                refund_status: enums::RefundStatus::from(item.response.status),
            }),
            ..item.data
        })
    }
}

impl TryFrom<RefundsResponseRouterData<RSync, NexinetsRefundResponse>>
    for RefundsRouterData<RSync>
{
    type Error = error_stack::Report<errors::ConnectorError>;
    fn try_from(
        item: RefundsResponseRouterData<RSync, NexinetsRefundResponse>,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            response: Ok(RefundsResponseData {
                connector_refund_id: item.response.transaction_id,
                refund_status: enums::RefundStatus::from(item.response.status),
            }),
            ..item.data
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NexinetsErrorResponse {
    pub status: u16,
    pub code: u16,
    pub message: String,
    pub errors: Vec<OrderErrorDetails>,
}

#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct OrderErrorDetails {
    pub code: u16,
    pub message: String,
    pub field: Option<String>,
}

fn get_payment_details_and_product(
    item: &PaymentsAuthorizeRouterData,
) -> Result<
    (Option<NexinetsPaymentDetails>, NexinetsProduct),
    error_stack::Report<errors::ConnectorError>,
> {
    match &item.request.payment_method_data {
        PaymentMethodData::Card(card) => Ok((
            Some(get_card_data(item, card)?),
            NexinetsProduct::Creditcard,
        )),
        PaymentMethodData::Wallet(wallet) => Ok(get_wallet_details(wallet)?),
        PaymentMethodData::BankRedirect(bank_redirect) => match bank_redirect {
            BankRedirectData::Eps { .. } => Ok((None, NexinetsProduct::Eps)),
            BankRedirectData::Giropay { .. } => Ok((None, NexinetsProduct::Giropay)),
            BankRedirectData::Ideal { bank_name, .. } => Ok((
                Some(NexinetsPaymentDetails::BankRedirects(Box::new(
                    NexinetsBankRedirects {
                        bic: bank_name
                            .map(|bank_name| NexinetsBIC::try_from(&bank_name))
                            .transpose()?,
                    },
                ))),
                NexinetsProduct::Ideal,
            )),
            BankRedirectData::Sofort { .. } => Ok((None, NexinetsProduct::Sofort)),
            BankRedirectData::BancontactCard { .. }
            | BankRedirectData::Blik { .. }
            | BankRedirectData::Bizum { .. }
            | BankRedirectData::Eft { .. }
            | BankRedirectData::Interac { .. }
            | BankRedirectData::OnlineBankingCzechRepublic { .. }
            | BankRedirectData::OnlineBankingFinland { .. }
            | BankRedirectData::OnlineBankingPoland { .. }
            | BankRedirectData::OnlineBankingSlovakia { .. }
            | BankRedirectData::OpenBankingUk { .. }
            | BankRedirectData::Przelewy24 { .. }
            | BankRedirectData::Trustly { .. }
            | BankRedirectData::OnlineBankingFpx { .. }
            | BankRedirectData::OnlineBankingThailand { .. }
            | BankRedirectData::LocalBankRedirect {} => {
                Err(errors::ConnectorError::NotImplemented(
                    utils::get_unimplemented_payment_method_error_message("nexinets"),
                ))?
            }
        },
        PaymentMethodData::CardRedirect(_)
        | PaymentMethodData::PayLater(_)
        | PaymentMethodData::BankDebit(_)
        | PaymentMethodData::BankTransfer(_)
        | PaymentMethodData::Crypto(_)
        | PaymentMethodData::MandatePayment
        | PaymentMethodData::Reward
        | PaymentMethodData::RealTimePayment(_)
        | PaymentMethodData::MobilePayment(_)
        | PaymentMethodData::Upi(_)
        | PaymentMethodData::Voucher(_)
        | PaymentMethodData::GiftCard(_)
        | PaymentMethodData::OpenBanking(_)
        | PaymentMethodData::CardToken(_)
        | PaymentMethodData::NetworkToken(_)
        | PaymentMethodData::CardDetailsForNetworkTransactionId(_) => {
            Err(errors::ConnectorError::NotImplemented(
                utils::get_unimplemented_payment_method_error_message("nexinets"),
            ))?
        }
    }
}

fn get_card_data(
    item: &PaymentsAuthorizeRouterData,
    card: &Card,
) -> Result<NexinetsPaymentDetails, errors::ConnectorError> {
    let (card_data, cof_contract) = match item.request.is_mandate_payment() {
        true => {
            let card_data = match item.request.off_session {
                Some(true) => CardDataDetails::PaymentInstrument(Box::new(PaymentInstrument {
                    payment_instrument_id: item.request.connector_mandate_id().map(Secret::new),
                })),
                _ => CardDataDetails::CardDetails(Box::new(get_card_details(card)?)),
            };
            let cof_contract = Some(CofContract {
                recurring_type: RecurringType::Unscheduled,
            });
            (card_data, cof_contract)
        }
        false => (
            CardDataDetails::CardDetails(Box::new(get_card_details(card)?)),
            None,
        ),
    };
    Ok(NexinetsPaymentDetails::Card(Box::new(NexiCardDetails {
        card_data,
        cof_contract,
    })))
}

fn get_applepay_details(
    wallet_data: &WalletData,
    applepay_data: &ApplePayWalletData,
) -> CustomResult<ApplePayDetails, errors::ConnectorError> {
    let payment_data = WalletData::get_wallet_token_as_json(wallet_data, "Apple Pay".to_string())?;
    Ok(ApplePayDetails {
        payment_data,
        payment_method: ApplepayPaymentMethod {
            display_name: applepay_data.payment_method.display_name.to_owned(),
            network: applepay_data.payment_method.network.to_owned(),
            token_type: applepay_data.payment_method.pm_type.to_owned(),
        },
        transaction_identifier: applepay_data.transaction_identifier.to_owned(),
    })
}

fn get_card_details(req_card: &Card) -> Result<CardDetails, errors::ConnectorError> {
    Ok(CardDetails {
        card_number: req_card.card_number.clone(),
        expiry_month: req_card.card_exp_month.clone(),
        expiry_year: req_card.get_card_expiry_year_2_digit()?,
        verification: req_card.card_cvc.clone(),
    })
}

fn get_wallet_details(
    wallet: &WalletData,
) -> Result<
    (Option<NexinetsPaymentDetails>, NexinetsProduct),
    error_stack::Report<errors::ConnectorError>,
> {
    match wallet {
        WalletData::PaypalRedirect(_) => Ok((None, NexinetsProduct::Paypal)),
        WalletData::ApplePay(applepay_data) => Ok((
            Some(NexinetsPaymentDetails::Wallet(Box::new(
                NexinetsWalletDetails::ApplePayToken(Box::new(get_applepay_details(
                    wallet,
                    applepay_data,
                )?)),
            ))),
            NexinetsProduct::Applepay,
        )),
        WalletData::AliPayQr(_)
        | WalletData::AliPayRedirect(_)
        | WalletData::AliPayHkRedirect(_)
        | WalletData::AmazonPayRedirect(_)
        | WalletData::Paysera(_)
        | WalletData::Skrill(_)
        | WalletData::BluecodeRedirect {}
        | WalletData::MomoRedirect(_)
        | WalletData::KakaoPayRedirect(_)
        | WalletData::GoPayRedirect(_)
        | WalletData::GcashRedirect(_)
        | WalletData::ApplePayRedirect(_)
        | WalletData::ApplePayThirdPartySdk(_)
        | WalletData::DanaRedirect { .. }
        | WalletData::GooglePay(_)
        | WalletData::GooglePayRedirect(_)
        | WalletData::GooglePayThirdPartySdk(_)
        | WalletData::MbWayRedirect(_)
        | WalletData::MobilePayRedirect(_)
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
        | WalletData::Mifinity(_)
        | WalletData::RevolutPay(_) => Err(errors::ConnectorError::NotImplemented(
            utils::get_unimplemented_payment_method_error_message("nexinets"),
        ))?,
    }
}

pub fn get_order_id(
    meta: &NexinetsPaymentsMetadata,
) -> Result<String, error_stack::Report<errors::ConnectorError>> {
    let order_id = meta.order_id.clone().ok_or(
        errors::ConnectorError::MissingConnectorRelatedTransactionID {
            id: "order_id".to_string(),
        },
    )?;
    Ok(order_id)
}

pub fn get_transaction_id(
    meta: &NexinetsPaymentsMetadata,
) -> Result<String, error_stack::Report<errors::ConnectorError>> {
    let transaction_id = meta.transaction_id.clone().ok_or(
        errors::ConnectorError::MissingConnectorRelatedTransactionID {
            id: "transaction_id".to_string(),
        },
    )?;
    Ok(transaction_id)
}
