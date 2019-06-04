use crate::schema::{current_height, merchants, rates, transactions};
use chrono::{Duration, NaiveDateTime, Utc};
use diesel::deserialize::{self, FromSql};
use diesel::pg::Pg;
use diesel::serialize::{self, Output, ToSql};
use diesel::sql_types::Jsonb;
use diesel_derive_enum::DbEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use strum_macros::{Display, EnumString};
use uuid::Uuid;

pub const NEW_PAYMENT_TTL_SECONDS: i64 = 15 * 60; //15 minutes since creation time
pub const PENDING_PAYMENT_TTL_SECONDS: i64 = 7 * 60; //7  minutes since became pending

pub const NEW_PAYOUT_TTL_SECONDS: i64 = 5 * 60; //5  minutes since creation time
pub const INITIALIZED_PAYOUT_TTL_SECONDS: i64 = 5 * 60; //5  minutes since creation time
pub const PENDING_PAYOUT_TTL_SECONDS: i64 = 15 * 60; //15 minutes since became pending

pub const WAIT_PER_CONFIRMATION_SECONDS: i64 = 5 * 60; // How long we wait per confirmation. E.g. if payment requires 5 confirmations we will wail 5 * WAIT_PER_CONFIRMATION_SECONDS

#[derive(Debug, Serialize, Deserialize, Queryable, Insertable, Identifiable, Clone)]
#[table_name = "merchants"]
pub struct Merchant {
    pub id: String,
    pub email: String,
    pub password: String,
    pub wallet_url: Option<String>,
    pub balance: i64,
    pub created_at: NaiveDateTime,
    pub token: String,
    pub callback_url: Option<String>,
    #[serde(skip_serializing)]
    pub token_2fa: Option<String>,
    #[serde(skip_serializing)]
    pub confirmed_2fa: bool,
}

/*
 * The status of payment changes flow is as follows:
 * New - transaction was created but no attempts were maid to pay
 * Pending - user sent a slate and we succesfully sent it to wallet
 * InChain - transaction was accepted to chain
 * Confirmed - we got required number of confirmation for this transaction
 * Rejected - transaction spent too much time in New or Pending state
 *
 * The status of payout changes as follows:
 * New - payout created in db
 * Initialized - we created transaction in wallet, created slate and sent it to merchant
 * Pending - user returned to us slate, we finalized it in wallet and wait for required number of confimations
 * Confirmed - we got required number of confimations
 */

#[derive(Debug, PartialEq, DbEnum, Serialize, Deserialize, Clone, Copy, EnumString, Display)]
#[DieselType = "Transaction_status"]
pub enum TransactionStatus {
    New,
    Pending,
    Rejected,
    InChain,
    Confirmed,
    Initialized,
    Refund,
}

#[derive(Debug, PartialEq, DbEnum, Serialize, Deserialize, Clone, Copy, EnumString, Display)]
#[DieselType = "Transaction_type"]
pub enum TransactionType {
    Payment,
    Payout,
}

#[derive(
    Debug, Serialize, Deserialize, Queryable, Insertable, Identifiable, Clone, AsExpression,
)]
#[table_name = "transactions"]
pub struct Transaction {
    pub id: Uuid,
    pub external_id: String,
    pub merchant_id: String,
    pub grin_amount: i64,
    pub amount: Money,
    pub status: TransactionStatus,
    pub confirmations: i64,
    pub email: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    #[serde(skip_serializing)]
    pub reported: bool,
    #[serde(skip_serializing)]
    pub report_attempts: i32,
    #[serde(skip_serializing)]
    pub next_report_attempt: Option<NaiveDateTime>,
    #[serde(skip_serializing)]
    pub wallet_tx_id: Option<i64>,
    #[serde(skip_serializing)]
    pub wallet_tx_slate_id: Option<String>,
    pub message: String,
    pub slate_messages: Option<Vec<String>>,
    pub knockturn_fee: Option<i64>,
    pub transfer_fee: Option<i64>,
    #[serde(skip_serializing)]
    pub real_transfer_fee: Option<i64>,
    pub transaction_type: TransactionType,
    #[serde(skip_serializing)]
    pub height: Option<i64>,
    #[serde(skip_serializing)]
    pub commit: Option<String>,
    pub redirect_url: Option<String>,
}

impl Transaction {
    pub fn is_expired(&self) -> bool {
        match self.time_until_expired() {
            Some(time) => time < Duration::zero(),
            None => false,
        }
    }

    pub fn time_until_expired(&self) -> Option<Duration> {
        let expiration_time = match (self.transaction_type, self.status) {
            (TransactionType::Payment, TransactionStatus::New) => {
                Some(self.created_at + Duration::seconds(NEW_PAYMENT_TTL_SECONDS))
            }
            (TransactionType::Payment, TransactionStatus::Pending) => {
                Some(self.updated_at + Duration::seconds(PENDING_PAYMENT_TTL_SECONDS))
            }
            (TransactionType::Payout, TransactionStatus::New) => {
                Some(self.created_at + Duration::seconds(NEW_PAYOUT_TTL_SECONDS))
            }
            (TransactionType::Payout, TransactionStatus::Initialized) => {
                Some(self.created_at + Duration::seconds(INITIALIZED_PAYOUT_TTL_SECONDS))
            }
            (TransactionType::Payout, TransactionStatus::Pending) => {
                Some(self.updated_at + Duration::seconds(PENDING_PAYOUT_TTL_SECONDS))
            }
            (_, TransactionStatus::InChain) => Some(
                self.updated_at
                    + Duration::seconds(self.confirmations * WAIT_PER_CONFIRMATION_SECONDS),
            ),
            (_, _) => None,
        };
        expiration_time.map(|exp_time| exp_time - Utc::now().naive_utc())
    }

    pub fn grins(&self) -> Money {
        Money::new(self.grin_amount, Currency::GRIN)
    }

    pub fn current_confirmations(&self, current_height: i64) -> i64 {
        match self.height {
            Some(height) => current_height - height,
            None => 0,
        }
    }

    pub fn is_invalid_amount(&self, payment_amount: u64) -> bool {
        let amount = self.grin_amount as u64;
        (payment_amount < amount) || (payment_amount - amount > 1_000_000)
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct Confirmation<'a> {
    pub id: &'a Uuid,
    pub token: &'a str,
    pub external_id: &'a str,
    pub merchant_id: &'a str,
    pub grin_amount: i64,
    pub amount: &'a Money,
    pub status: TransactionStatus,
    pub confirmations: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum Currency {
    GRIN = 0,
    BTC = 1,
    EUR = 2,
    USD = 3,
}

impl Currency {
    pub fn precision(&self) -> i64 {
        match self {
            Currency::BTC => 100_000_000,
            Currency::GRIN => 1_000_000_000,
            Currency::EUR | Currency::USD => 100,
        }
    }

    fn symbol(&self) -> &'static str {
        match self {
            Currency::BTC => "BTC",
            Currency::GRIN => "ツ",
            Currency::EUR => "€",
            Currency::USD => "$",
        }
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            Currency::BTC => s!("BTC"),
            Currency::GRIN => s!("GRIN"),
            Currency::EUR => s!("EUR"),
            Currency::USD => s!("USD"),
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Serialize, Deserialize, AsExpression, FromSqlRow, Clone, Copy)]
#[sql_type = "Jsonb"]
pub struct Money {
    pub amount: i64,
    pub currency: Currency,
}

impl From<i64> for Money {
    fn from(val: i64) -> Money {
        Money::from_grin(val)
    }
}

impl Money {
    pub fn new(amount: i64, currency: Currency) -> Self {
        Money { amount, currency }
    }

    pub fn from_grin(amount: i64) -> Self {
        Money {
            amount: amount,
            currency: Currency::GRIN,
        }
    }

    pub fn convert_to(&self, currency: Currency, rate: f64) -> Money {
        let amount =
            self.amount * currency.precision() / (self.currency.precision() as f64 * rate) as i64;
        Money {
            amount,
            currency: currency,
        }
    }

    pub fn amount(&self) -> String {
        let pr = self.currency.precision();
        let grins = self.amount / pr;
        let mgrins = self.amount % pr;
        match self.currency {
            Currency::BTC => format!("{}.{:08}", grins, mgrins),
            Currency::GRIN => {
                let short = (mgrins as f64 / 1_000_000.0).ceil() as i64;
                format!("{}.{:03}", grins, short)
            }
            _ => format!("{}.{:02}", grins, mgrins),
        }
    }
}

impl ToSql<Jsonb, Pg> for Money {
    fn to_sql<W: std::io::Write>(&self, out: &mut Output<W, Pg>) -> serialize::Result {
        out.write_all(&[1])?;
        serde_json::to_writer(out, self)
            .map(|_| serialize::IsNull::No)
            .map_err(Into::into)
    }
}

impl FromSql<Jsonb, Pg> for Money {
    fn from_sql(bytes: Option<&[u8]>) -> deserialize::Result<Self> {
        let bytes = not_none!(bytes);
        if bytes[0] != 1 {
            return Err("Unsupported JSONB encoding version".into());
        }
        serde_json::from_slice(&bytes[1..]).map_err(Into::into)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.amount(), self.currency.symbol())
    }
}

#[derive(Debug, Serialize, Deserialize, Queryable, Insertable, Identifiable, AsChangeset)]
#[table_name = "rates"]
pub struct Rate {
    pub id: String,
    pub rate: f64,
    pub updated_at: NaiveDateTime,
}
#[derive(Debug, Serialize, Deserialize, Queryable, Insertable)]
#[table_name = "current_height"]
pub struct CurrentHeight {
    pub height: i64,
}

#[cfg(test)]
mod tests {

    use crate::models::*;
    fn create_tx() -> Transaction {
        Transaction {
            id: Uuid::new_v4(),
            external_id: s!(""),
            merchant_id: s!(""),
            grin_amount: 1_000_000_000,
            amount: Money::from_grin(1_000_000),
            status: TransactionStatus::New,
            confirmations: 3,
            email: None,
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            reported: false,
            report_attempts: 0,
            next_report_attempt: None,
            wallet_tx_id: None,
            wallet_tx_slate_id: None,
            message: s!("msg"),
            slate_messages: None,
            knockturn_fee: None,
            transfer_fee: None,
            real_transfer_fee: None,
            transaction_type: TransactionType::Payment,
            height: None,
            commit: None,
            redirect_url: Some(s!("https://store.cycle42.com")),
        }
    }

    fn approximately(expect: i64, real: i64) -> bool {
        let ratio = expect as f64 / real as f64;
        ratio > 0.99 && ratio < 1.01
    }
    #[test]
    fn test_expiration_date() {
        let mut tx = create_tx();
        tx.status = TransactionStatus::New;
        assert!(approximately(
            tx.time_until_expired().unwrap().num_seconds(),
            NEW_PAYMENT_TTL_SECONDS
        ));
        tx.status = TransactionStatus::Pending;
        assert!(approximately(
            tx.time_until_expired().unwrap().num_seconds(),
            PENDING_PAYMENT_TTL_SECONDS
        ));

        tx.status = TransactionStatus::Confirmed;
        assert!(tx.time_until_expired() == None);

        tx.transaction_type = TransactionType::Payout;
        tx.status = TransactionStatus::New;
        assert!(approximately(
            tx.time_until_expired().unwrap().num_seconds(),
            NEW_PAYOUT_TTL_SECONDS
        ));
        tx.status = TransactionStatus::Initialized;
        assert!(approximately(
            tx.time_until_expired().unwrap().num_seconds(),
            INITIALIZED_PAYOUT_TTL_SECONDS
        ));
        tx.status = TransactionStatus::Pending;
        assert!(approximately(
            tx.time_until_expired().unwrap().num_seconds(),
            PENDING_PAYOUT_TTL_SECONDS
        ));
        tx.status = TransactionStatus::Confirmed;
        assert!(tx.time_until_expired() == None);
    }

    #[test]
    fn test_money_amount() {
        let mut m = Money::new(1000, Currency::EUR);
        assert_eq!(&m.amount(), "10.00");
        m = Money::new(2_000_000_01, Currency::BTC);
        assert_eq!(&m.amount(), "2.00000001");
        m = Money::new(2_000_000_01, Currency::GRIN);
        assert_eq!(&m.amount(), "0.201");
    }

    #[test]
    fn test_pay_invalid_amount() {
        let tx = create_tx();
        assert!(tx.is_invalid_amount(100));
        assert!(!tx.is_invalid_amount(1_000_000_000));
        assert!(tx.is_invalid_amount(999_999_999));
        assert!(tx.is_invalid_amount(1_999_999_999));
        assert!(tx.is_invalid_amount(1_002_000_000));
        assert!(!tx.is_invalid_amount(1_000_100_000));
    }
}
