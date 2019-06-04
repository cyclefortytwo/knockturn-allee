use crate::errors::*;
use crate::models::{
    Currency, Merchant, Money, Rate, Transaction, TransactionStatus, TransactionType,
    NEW_PAYMENT_TTL_SECONDS,
};
use actix::{Actor, SyncContext};
use actix::{Handler, Message};
use chrono::NaiveDateTime;
use chrono::{Duration, Local, Utc};
use data_encoding::BASE32;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{self, prelude::*};
use log::info;
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

const MAX_REPORT_ATTEMPTS: i32 = 10; //Number or attemps we try to run merchant's callback

pub struct DbExecutor(pub Pool<ConnectionManager<PgConnection>>);

impl Actor for DbExecutor {
    type Context = SyncContext<Self>;
}

#[derive(Debug, Deserialize)]
pub struct CreateMerchant {
    pub id: String,
    pub email: String,
    pub password: String,
    pub wallet_url: Option<String>,
    pub callback_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetMerchant {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct GetTransaction {
    pub transaction_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GetTransactions {
    pub merchant_id: String,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateTransaction {
    pub merchant_id: String,
    pub external_id: String,
    pub amount: Money,
    pub confirmations: i64,
    pub email: Option<String>,
    pub message: String,
    pub transaction_type: TransactionType,
    pub redirect_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTransactionStatus {
    pub id: Uuid,
    pub status: TransactionStatus,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRate {
    pub rates: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct ConvertCurrency {
    pub amount: Money,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct GetPayment {
    pub transaction_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct GetPaymentsByStatus(pub TransactionStatus);

#[derive(Debug, Deserialize)]
pub struct GetPayoutsByStatus(pub TransactionStatus);

pub struct ConfirmTransaction {
    pub transaction: Transaction,
    pub confirmed_at: Option<NaiveDateTime>,
}

#[derive(Debug, Deserialize)]
pub struct ReportAttempt {
    pub transaction_id: Uuid,
    pub next_attempt: Option<NaiveDateTime>,
}

#[derive(Debug, Deserialize)]
pub struct GetUnreportedPaymentsByStatus(pub TransactionStatus);

#[derive(Debug, Deserialize)]
pub struct Confirm2FA {
    pub merchant_id: String,
}

#[derive(Debug, Deserialize)]
pub struct Reset2FA {
    pub merchant_id: String,
}

#[derive(Debug, Deserialize)]
pub struct GetCurrentHeight;

#[derive(Debug, Deserialize)]
pub struct RejectExpiredPayments;

impl Message for CreateMerchant {
    type Result = Result<Merchant, Error>;
}

impl Message for GetMerchant {
    type Result = Result<Merchant, Error>;
}

impl Message for GetTransaction {
    type Result = Result<Transaction, Error>;
}

impl Message for GetPayment {
    type Result = Result<Transaction, Error>;
}

impl Message for GetPaymentsByStatus {
    type Result = Result<Vec<Transaction>, Error>;
}

impl Message for GetPayoutsByStatus {
    type Result = Result<Vec<Transaction>, Error>;
}

impl Message for GetTransactions {
    type Result = Result<Vec<Transaction>, Error>;
}

impl Message for CreateTransaction {
    type Result = Result<Transaction, Error>;
}

impl Message for UpdateTransactionStatus {
    type Result = Result<Transaction, Error>;
}

impl Message for RegisterRate {
    type Result = Result<(), Error>;
}

impl Message for ConvertCurrency {
    type Result = Result<Money, Error>;
}
impl Message for ConfirmTransaction {
    type Result = Result<Transaction, Error>;
}

impl Message for ReportAttempt {
    type Result = Result<(), Error>;
}

impl Message for GetUnreportedPaymentsByStatus {
    type Result = Result<Vec<Transaction>, Error>;
}

impl Message for Confirm2FA {
    type Result = Result<(), Error>;
}

impl Message for Reset2FA {
    type Result = Result<(), Error>;
}

impl Message for RejectExpiredPayments {
    type Result = Result<(), Error>;
}

impl Message for GetCurrentHeight {
    type Result = Result<i64, Error>;
}

impl Handler<CreateMerchant> for DbExecutor {
    type Result = Result<Merchant, Error>;

    fn handle(&mut self, msg: CreateMerchant, _: &mut Self::Context) -> Self::Result {
        use crate::schema::merchants::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
    abcdefghijklmnopqrstuvwxyz\
    0123456789";

        let mut rng = thread_rng();
        let new_token: Option<String> = (0..64)
            .map(|_| Some(*CHARSET.choose(&mut rng)? as char))
            .collect();
        let new_token_2fa = BASE32.encode(&rng.gen::<[u8; 10]>());
        let new_merchant = Merchant {
            id: msg.id,
            email: msg.email,
            password: msg.password,
            wallet_url: msg.wallet_url,
            balance: 0,
            created_at: Local::now().naive_local() + Duration::hours(24),
            callback_url: msg.callback_url,
            token: new_token.ok_or(Error::General(s!("cannot generate rangom token")))?,
            token_2fa: Some(new_token_2fa),
            confirmed_2fa: false,
        };

        diesel::insert_into(merchants)
            .values(&new_merchant)
            .get_result(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<GetMerchant> for DbExecutor {
    type Result = Result<Merchant, Error>;

    fn handle(&mut self, msg: GetMerchant, _: &mut Self::Context) -> Self::Result {
        use crate::schema::merchants::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        merchants
            .find(msg.id)
            .get_result(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<GetTransaction> for DbExecutor {
    type Result = Result<Transaction, Error>;

    fn handle(&mut self, msg: GetTransaction, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        transactions
            .find(msg.transaction_id)
            .get_result(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<GetPayment> for DbExecutor {
    type Result = Result<Transaction, Error>;

    fn handle(&mut self, msg: GetPayment, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        transactions
            .filter(id.eq(msg.transaction_id))
            .filter(transaction_type.eq(TransactionType::Payment))
            .get_result(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<GetPaymentsByStatus> for DbExecutor {
    type Result = Result<Vec<Transaction>, Error>;

    fn handle(&mut self, msg: GetPaymentsByStatus, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        transactions
            .filter(transaction_type.eq(TransactionType::Payment))
            .filter(status.eq(msg.0))
            .load::<Transaction>(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<GetPayoutsByStatus> for DbExecutor {
    type Result = Result<Vec<Transaction>, Error>;

    fn handle(&mut self, msg: GetPayoutsByStatus, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        transactions
            .filter(transaction_type.eq(TransactionType::Payout))
            .filter(status.eq(msg.0))
            .load::<Transaction>(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<GetTransactions> for DbExecutor {
    type Result = Result<Vec<Transaction>, Error>;

    fn handle(&mut self, msg: GetTransactions, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        transactions
            .filter(merchant_id.eq(msg.merchant_id))
            .offset(msg.offset)
            .limit(msg.limit)
            .load::<Transaction>(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<CreateTransaction> for DbExecutor {
    type Result = Result<Transaction, Error>;

    fn handle(&mut self, msg: CreateTransaction, _: &mut Self::Context) -> Self::Result {
        use crate::schema::merchants::dsl::*;
        use crate::schema::rates::dsl::*;
        use crate::schema::transactions::dsl::*;

        let conn: &PgConnection = &self.0.get().unwrap();

        if !merchants
            .find(msg.merchant_id.clone())
            .get_result::<Merchant>(conn)
            .is_ok()
        {
            return Err(Error::InvalidEntity("merchant".to_owned()));
        }

        let exch_rate = match rates
            .find(&msg.amount.currency.to_string())
            .get_result::<Rate>(conn)
            .optional()?
        {
            None => return Err(Error::UnsupportedCurrency(msg.amount.currency.to_string())),
            Some(v) => v,
        };

        let grins = msg.amount.convert_to(Currency::GRIN, exch_rate.rate);

        let new_transaction = Transaction {
            id: uuid::Uuid::new_v4(),
            external_id: msg.external_id,
            merchant_id: msg.merchant_id,
            email: msg.email,
            amount: msg.amount,
            grin_amount: grins.amount,
            status: TransactionStatus::New,
            confirmations: msg.confirmations,
            created_at: Local::now().naive_local(),
            updated_at: Local::now().naive_local(),
            report_attempts: 0,
            next_report_attempt: None,
            reported: false,
            wallet_tx_id: None,
            wallet_tx_slate_id: None,
            message: msg.message,
            slate_messages: None,
            transfer_fee: None,
            knockturn_fee: None,
            real_transfer_fee: None,
            transaction_type: msg.transaction_type,
            height: None,
            commit: None,
            redirect_url: msg.redirect_url,
        };

        diesel::insert_into(transactions)
            .values(&new_transaction)
            .get_result(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<UpdateTransactionStatus> for DbExecutor {
    type Result = Result<Transaction, Error>;

    fn handle(&mut self, msg: UpdateTransactionStatus, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();

        diesel::update(transactions.filter(id.eq(msg.id)))
            .set((status.eq(msg.status), updated_at.eq(Utc::now().naive_utc())))
            .get_result(conn)
            .map_err(|e| e.into())
    }
}

impl Handler<RegisterRate> for DbExecutor {
    type Result = Result<(), Error>;

    fn handle(&mut self, msg: RegisterRate, _: &mut Self::Context) -> Self::Result {
        use crate::schema::rates::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();

        for (currency, new_rate) in msg.rates {
            let new_rate = Rate {
                id: currency.to_uppercase(),
                rate: new_rate,
                updated_at: Local::now().naive_local(),
            };

            diesel::insert_into(rates)
                .values(&new_rate)
                .on_conflict(id)
                .do_update()
                .set(&new_rate)
                .get_result::<Rate>(conn)
                .map_err(|e| Error::from(e))?;
        }
        Ok(())
    }
}

impl Handler<ConfirmTransaction> for DbExecutor {
    type Result = Result<Transaction, Error>;

    fn handle(&mut self, msg: ConfirmTransaction, _: &mut Self::Context) -> Self::Result {
        use crate::schema::merchants;
        use crate::schema::transactions;
        let conn: &PgConnection = &self.0.get().unwrap();

        conn.transaction(|| {
            let tx = diesel::update(
                transactions::table.filter(transactions::columns::id.eq(msg.transaction.id)),
            )
            .set((
                transactions::columns::status.eq(TransactionStatus::Confirmed),
                transactions::columns::updated_at.eq(Utc::now().naive_utc()),
            ))
            .get_result(conn)?;
            diesel::update(
                merchants::table.filter(merchants::columns::id.eq(msg.transaction.merchant_id)),
            )
            .set(
                merchants::columns::balance
                    .eq(merchants::columns::balance + msg.transaction.grin_amount),
            )
            .get_result(conn)
            .map(|_: Merchant| ())?;
            Ok(tx)
        })
    }
}

impl Handler<ReportAttempt> for DbExecutor {
    type Result = Result<(), Error>;

    fn handle(&mut self, msg: ReportAttempt, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        let next_attempt = msg
            .next_attempt
            .unwrap_or(Utc::now().naive_utc() + Duration::seconds(10));
        diesel::update(transactions.filter(id.eq(msg.transaction_id)))
            .set((
                report_attempts.eq(report_attempts + 1),
                next_report_attempt.eq(next_attempt),
            ))
            .get_result(conn)
            .map_err(|e| e.into())
            .map(|_: Transaction| ())
    }
}

impl Handler<GetUnreportedPaymentsByStatus> for DbExecutor {
    type Result = Result<Vec<Transaction>, Error>;

    fn handle(
        &mut self,
        msg: GetUnreportedPaymentsByStatus,
        _: &mut Self::Context,
    ) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();

        let query = transactions
            .filter(reported.ne(true))
            .filter(status.eq(msg.0))
            .filter(report_attempts.lt(MAX_REPORT_ATTEMPTS))
            .filter(
                next_report_attempt
                    .le(Utc::now().naive_utc())
                    .or(next_report_attempt.is_null()),
            );

        let payments = query
            .load::<Transaction>(conn)
            .map_err(|e| Error::Db(s!(e)))?;

        Ok(payments)
    }
}

impl Handler<Confirm2FA> for DbExecutor {
    type Result = Result<(), Error>;

    fn handle(&mut self, msg: Confirm2FA, _: &mut Self::Context) -> Self::Result {
        info!("Confirm 2fa token for merchant {}", msg.merchant_id);
        use crate::schema::merchants::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        diesel::update(merchants.filter(id.eq(msg.merchant_id)))
            .set((confirmed_2fa.eq(true),))
            .get_result(conn)
            .map_err(|e| e.into())
            .map(|_: Merchant| ())
    }
}

impl Handler<Reset2FA> for DbExecutor {
    type Result = Result<(), Error>;

    fn handle(&mut self, msg: Reset2FA, _: &mut Self::Context) -> Self::Result {
        info!("Confirm 2fa token for merchant {}", msg.merchant_id);
        use crate::schema::merchants::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();

        let new_token_2fa = BASE32.encode(&thread_rng().gen::<[u8; 10]>());
        diesel::update(merchants.filter(id.eq(msg.merchant_id)))
            .set((confirmed_2fa.eq(false), token_2fa.eq(new_token_2fa)))
            .get_result(conn)
            .map_err(|e| e.into())
            .map(|_: Merchant| ())
    }
}

impl Handler<RejectExpiredPayments> for DbExecutor {
    type Result = Result<(), Error>;

    fn handle(&mut self, _: RejectExpiredPayments, _: &mut Self::Context) -> Self::Result {
        use crate::schema::transactions::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        diesel::update(
            transactions
                .filter(status.eq(TransactionStatus::New))
                .filter(transaction_type.eq(TransactionType::Payment))
                .filter(
                    created_at
                        .lt(Utc::now().naive_utc() - Duration::seconds(NEW_PAYMENT_TTL_SECONDS)),
                ),
        )
        .set(status.eq(TransactionStatus::Rejected))
        .execute(conn)
        .map_err(|e| e.into())
        .map(|n| {
            if n > 0 {
                info!("Rejected {} expired new payments", n);
            }
            ()
        })
    }
}
impl Handler<GetCurrentHeight> for DbExecutor {
    type Result = Result<i64, Error>;

    fn handle(&mut self, _: GetCurrentHeight, _: &mut Self::Context) -> Self::Result {
        use crate::schema::current_height::dsl::*;
        let conn: &PgConnection = &self.0.get().unwrap();
        current_height
            .select(height)
            .first(conn)
            .map_err(|e| e.into())
    }
}
