use crate::blocking;
use crate::db::{
    self, CreateTransaction, DbExecutor, GetMerchant, GetPayment, MarkAsReported, ReportAttempt,
    UpdateTransactionStatus,
};
use crate::errors::Error;
use crate::models::{Confirmation, Money, Transaction, TransactionStatus};
use crate::wallet::TxLogEntry;
use crate::wallet::Wallet;
use actix::{Actor, Addr, Context, Handler, Message, ResponseFuture};
use actix_web::client;
use chrono::{Duration, Utc};
use derive_deref::Deref;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{self, prelude::*};
use futures::future::{Either, Future};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use uuid::Uuid;

pub struct Fsm {
    pub db: Addr<DbExecutor>,
    pub wallet: Wallet,
    pub pool: Pool<ConnectionManager<PgConnection>>,
}

impl Actor for Fsm {
    type Context = Context<Self>;
}

/*
 * These are messages to control Payments State Machine
 *
 */

#[derive(Debug, Serialize, Deserialize, Clone, Deref)]
pub struct NewPayment(Transaction);

#[derive(Debug, Deserialize, Clone, Deref)]
pub struct PendingPayment(Transaction);

#[derive(Debug, Deserialize, Clone, Deref)]
pub struct InChainPayment(Transaction);

#[derive(Debug, Deserialize, Clone, Deref, Serialize)]
pub struct ConfirmedPayment(Transaction);

#[derive(Debug, Deserialize, Clone, Deref)]
pub struct RejectedPayment(Transaction);

#[derive(Debug, Deserialize, Clone, Deref)]
pub struct UnreportedPayment(Transaction);

#[derive(Debug, Deserialize)]
pub struct CreatePayment {
    pub merchant_id: String,
    pub external_id: String,
    pub amount: Money,
    pub confirmations: i64,
    pub email: Option<String>,
    pub message: String,
    pub redirect_url: Option<String>,
}

impl Message for CreatePayment {
    type Result = Result<NewPayment, Error>;
}

#[derive(Debug, Deserialize)]
pub struct MakePayment {
    pub new_payment: NewPayment,
    pub wallet_tx: TxLogEntry,
    pub commit: Vec<u8>,
}

impl Message for MakePayment {
    type Result = Result<PendingPayment, Error>;
}

#[derive(Debug, Deserialize)]
pub struct ConfirmPayment {
    pub payment: InChainPayment,
}

impl Message for ConfirmPayment {
    type Result = Result<ConfirmedPayment, Error>;
}

#[derive(Debug, Deserialize, Deref)]
pub struct RejectPayment<T> {
    pub payment: T,
}

impl Message for RejectPayment<NewPayment> {
    type Result = Result<RejectedPayment, Error>;
}

impl Message for RejectPayment<PendingPayment> {
    type Result = Result<RejectedPayment, Error>;
}

#[derive(Debug, Deserialize, Deref)]
pub struct ReportPayment<T> {
    pub payment: T,
}

impl Message for ReportPayment<ConfirmedPayment> {
    type Result = Result<(), Error>;
}

impl Message for ReportPayment<RejectedPayment> {
    type Result = Result<(), Error>;
}

impl Message for ReportPayment<UnreportedPayment> {
    type Result = Result<(), Error>;
}

#[derive(Debug, Deserialize)]
pub struct GetNewPayment {
    pub transaction_id: Uuid,
}

impl Message for GetNewPayment {
    type Result = Result<NewPayment, Error>;
}

#[derive(Debug, Deserialize)]
pub struct GetPendingPayments;

impl Message for GetPendingPayments {
    type Result = Result<Vec<PendingPayment>, Error>;
}

#[derive(Debug, Deserialize)]
pub struct GetConfirmedPayments;

impl Message for GetConfirmedPayments {
    type Result = Result<Vec<ConfirmedPayment>, Error>;
}

#[derive(Debug, Deserialize)]
pub struct GetUnreportedPayments;

impl Message for GetUnreportedPayments {
    type Result = Result<Vec<UnreportedPayment>, Error>;
}

impl Handler<CreatePayment> for Fsm {
    type Result = ResponseFuture<NewPayment, Error>;

    fn handle(&mut self, msg: CreatePayment, _: &mut Self::Context) -> Self::Result {
        let create_transaction = CreateTransaction {
            merchant_id: msg.merchant_id,
            external_id: msg.external_id,
            amount: msg.amount,
            confirmations: msg.confirmations,
            email: msg.email.clone(),
            message: msg.message.clone(),
            redirect_url: msg.redirect_url,
        };

        let res = self
            .db
            .send(create_transaction)
            .from_err()
            .and_then(move |db_response| {
                let transaction = db_response?;
                Ok(NewPayment(transaction))
            });
        Box::new(res)
    }
}

impl Handler<GetNewPayment> for Fsm {
    type Result = ResponseFuture<NewPayment, Error>;

    fn handle(&mut self, msg: GetNewPayment, _: &mut Self::Context) -> Self::Result {
        let res = self
            .db
            .send(GetPayment {
                transaction_id: msg.transaction_id,
            })
            .from_err()
            .and_then(move |db_response| {
                let transaction = db_response?;
                if transaction.status != TransactionStatus::New {
                    return Err(Error::WrongTransactionStatus(s!(transaction.status)));
                }
                Ok(NewPayment(transaction))
            });
        Box::new(res)
    }
}

pub fn to_hex(bytes: Vec<u8>) -> String {
    let mut s = String::new();
    for byte in bytes {
        write!(&mut s, "{:02x}", byte).expect("Unable to write");
    }
    s
}

impl Handler<MakePayment> for Fsm {
    type Result = ResponseFuture<PendingPayment, Error>;

    fn handle(&mut self, msg: MakePayment, _: &mut Self::Context) -> Self::Result {
        let transaction_id = msg.new_payment.id.clone();
        let wallet_tx = msg.wallet_tx.clone();
        let messages: Option<Vec<String>> = wallet_tx.messages.map(|pm| {
            pm.messages
                .into_iter()
                .map(|pmd| pmd.message)
                .filter_map(|x| x)
                .collect()
        });

        let pool = self.pool.clone();

        let res = blocking::run(move || {
            use crate::schema::transactions::dsl::*;
            let conn: &PgConnection = &pool.get().unwrap();

            let transaction = diesel::update(transactions.filter(id.eq(transaction_id.clone())))
                .set((
                    wallet_tx_id.eq(msg.wallet_tx.id as i64),
                    wallet_tx_slate_id.eq(msg.wallet_tx.tx_slate_id.unwrap()),
                    slate_messages.eq(messages),
                    real_transfer_fee.eq(msg.wallet_tx.fee.map(|fee| fee as i64)),
                    status.eq(TransactionStatus::Pending),
                    commit.eq(to_hex(msg.commit)),
                ))
                .get_result(conn)
                .map_err::<Error, _>(|e| e.into())?;
            Ok(PendingPayment(transaction))
        })
        .from_err();

        Box::new(res)
    }
}

impl Handler<GetPendingPayments> for Fsm {
    type Result = ResponseFuture<Vec<PendingPayment>, Error>;

    fn handle(&mut self, _: GetPendingPayments, _: &mut Self::Context) -> Self::Result {
        Box::new(
            self.db
                .send(db::GetPaymentsByStatus(TransactionStatus::Pending))
                .from_err()
                .and_then(|db_response| {
                    let data = db_response?;
                    Ok(data.into_iter().map(PendingPayment).collect())
                }),
        )
    }
}

impl Handler<ConfirmPayment> for Fsm {
    type Result = ResponseFuture<ConfirmedPayment, Error>;

    fn handle(&mut self, msg: ConfirmPayment, _: &mut Self::Context) -> Self::Result {
        let tx_msg = db::ConfirmTransaction {
            transaction: msg.payment.0,
            confirmed_at: Some(Utc::now().naive_utc()),
        };
        Box::new(self.db.send(tx_msg).from_err().and_then(|res| {
            let tx = res?;
            Ok(ConfirmedPayment(tx))
        }))
    }
}

impl Handler<GetConfirmedPayments> for Fsm {
    type Result = ResponseFuture<Vec<ConfirmedPayment>, Error>;

    fn handle(&mut self, _: GetConfirmedPayments, _: &mut Self::Context) -> Self::Result {
        Box::new(
            self.db
                .send(db::GetPaymentsByStatus(TransactionStatus::Confirmed))
                .from_err()
                .and_then(|db_response| {
                    let data = db_response?;
                    Ok(data.into_iter().map(ConfirmedPayment).collect())
                }),
        )
    }
}

fn run_callback(
    callback_url: &str,
    token: &String,
    transaction: Transaction,
) -> impl Future<Item = (), Error = Error> {
    client::post(callback_url)
        .json(Confirmation {
            id: transaction.id,
            external_id: transaction.external_id,
            merchant_id: transaction.merchant_id,
            grin_amount: transaction.grin_amount,
            amount: transaction.amount,
            status: transaction.status,
            confirmations: transaction.confirmations,
            token: token.to_string(),
        })
        .unwrap()
        .send()
        .map_err({
            let callback_url = callback_url.to_owned();
            move |e| Error::MerchantCallbackError {
                callback_url: callback_url,
                error: s!(e),
            }
        })
        .and_then({
            let callback_url = callback_url.to_owned();
            |resp| {
                if resp.status().is_success() {
                    Ok(())
                } else {
                    Err(Error::MerchantCallbackError {
                        callback_url: callback_url,
                        error: s!("aaa"),
                    })
                }
            }
        })
}

impl Handler<RejectPayment<NewPayment>> for Fsm {
    type Result = ResponseFuture<RejectedPayment, Error>;

    fn handle(&mut self, msg: RejectPayment<NewPayment>, _: &mut Self::Context) -> Self::Result {
        Box::new(reject_transaction(&self.db, &msg.payment.id).map(RejectedPayment))
    }
}

impl Handler<RejectPayment<PendingPayment>> for Fsm {
    type Result = ResponseFuture<RejectedPayment, Error>;

    fn handle(
        &mut self,
        msg: RejectPayment<PendingPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(reject_transaction(&self.db, &msg.payment.id).map(RejectedPayment))
    }
}

fn reject_transaction(
    db: &Addr<DbExecutor>,
    id: &Uuid,
) -> impl Future<Item = Transaction, Error = Error> {
    db.send(UpdateTransactionStatus {
        id: id.clone(),
        status: TransactionStatus::Rejected,
    })
    .from_err()
    .and_then(|db_response| {
        let tx = db_response?;
        Ok(tx)
    })
}

impl Handler<ReportPayment<ConfirmedPayment>> for Fsm {
    type Result = ResponseFuture<(), Error>;

    fn handle(
        &mut self,
        msg: ReportPayment<ConfirmedPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(report_transaction(self.db.clone(), msg.payment.0))
    }
}

impl Handler<ReportPayment<RejectedPayment>> for Fsm {
    type Result = ResponseFuture<(), Error>;

    fn handle(
        &mut self,
        msg: ReportPayment<RejectedPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(report_transaction(self.db.clone(), msg.payment.0))
    }
}

impl Handler<ReportPayment<UnreportedPayment>> for Fsm {
    type Result = ResponseFuture<(), Error>;

    fn handle(
        &mut self,
        msg: ReportPayment<UnreportedPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(report_transaction(self.db.clone(), msg.payment.0))
    }
}

fn report_transaction(
    db: Addr<DbExecutor>,
    transaction: Transaction,
) -> impl Future<Item = (), Error = Error> {
    debug!("Try to report transaction {}", transaction.id);
    db.send(GetMerchant {
        id: transaction.merchant_id.clone(),
    })
    .from_err()
    .and_then(|res| {
        let merchant = res?;
        Ok(merchant)
    })
    .and_then(move |merchant| {
        if let Some(callback_url) = merchant.callback_url.clone() {
            debug!("Run callback for merchant {}", merchant.email);
            let res = run_callback(&callback_url, &merchant.token, transaction.clone())
                .or_else({
                    let db = db.clone();
                    let transaction = transaction.clone();
                    move |callback_err| {
                        // try call ReportAttempt but ignore errors and return
                        // error from callback
                        let next_attempt = Utc::now().naive_utc()
                            + Duration::seconds(
                                10 * (transaction.report_attempts + 1).pow(2) as i64,
                            );
                        db.send(ReportAttempt {
                            transaction_id: transaction.id,
                            next_attempt: Some(next_attempt),
                        })
                        .map_err(|e| Error::General(s!(e)))
                        .and_then(|db_response| {
                            db_response?;
                            Ok(())
                        })
                        .or_else(|e| {
                            error!("Get error in ReportAttempt {}", e);
                            Ok(())
                        })
                        .and_then(|_| Err(callback_err))
                    }
                })
                .and_then({
                    let db = db.clone();
                    let transaction_id = transaction.id.clone();
                    move |_| {
                        db.send(MarkAsReported { transaction_id })
                            .from_err()
                            .and_then(|db_response| {
                                db_response?;
                                Ok(())
                            })
                    }
                });
            Either::A(res)
        } else {
            Either::B(
                db.send(MarkAsReported {
                    transaction_id: transaction.id,
                })
                .from_err()
                .and_then(|db_response| {
                    db_response?;
                    Ok(())
                }),
            )
        }
    })
}

impl Handler<GetUnreportedPayments> for Fsm {
    type Result = ResponseFuture<Vec<UnreportedPayment>, Error>;

    fn handle(&mut self, _: GetUnreportedPayments, _: &mut Self::Context) -> Self::Result {
        Box::new(
            self.db
                .send(db::GetUnreportedTransactions)
                .from_err()
                .and_then(|db_response| {
                    let data = db_response?;
                    Ok(data.into_iter().map(UnreportedPayment).collect())
                }),
        )
    }
}
