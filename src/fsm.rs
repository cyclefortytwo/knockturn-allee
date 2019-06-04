use crate::blocking;
use crate::db::{
    self, CreateTransaction, DbExecutor, GetMerchant, GetPayment, GetUnreportedPaymentsByStatus,
    ReportAttempt, UpdateTransactionStatus,
};
use crate::errors::Error;
use crate::models::Merchant;
use crate::models::{Confirmation, Money, Transaction, TransactionStatus, TransactionType};
use crate::ser;
use crate::wallet::TxLogEntry;
use crate::wallet::Wallet;
use actix::{Actor, Addr, Context, Handler, Message, ResponseFuture};
use actix_web::client;
use chrono::{Duration, Utc};
use derive_deref::Deref;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{self, prelude::*};
use futures::future::{ok, Either, Future};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MINIMAL_WITHDRAW: i64 = 1_000_000_000;
pub const KNOCKTURN_SHARE: f64 = 0.01;
pub const TRANSFER_FEE: i64 = 8_000_000;

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
pub struct RefundPayment(Transaction);

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
pub struct SeenInChainPayment<T> {
    pub payment: T,
    pub height: i64,
}

impl Message for SeenInChainPayment<PendingPayment> {
    type Result = Result<InChainPayment, Error>;
}

impl Message for SeenInChainPayment<RejectedPayment> {
    type Result = Result<RefundPayment, Error>;
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
pub struct GetUnreportedConfirmedPayments;

impl Message for GetUnreportedConfirmedPayments {
    type Result = Result<Vec<ConfirmedPayment>, Error>;
}

#[derive(Debug, Deserialize)]
pub struct GetUnreportedRejectedPayments;

impl Message for GetUnreportedRejectedPayments {
    type Result = Result<Vec<RejectedPayment>, Error>;
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
            transaction_type: TransactionType::Payment,
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
                    commit.eq(ser::to_hex(msg.commit)),
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

impl Handler<SeenInChainPayment<PendingPayment>> for Fsm {
    type Result = ResponseFuture<InChainPayment, Error>;

    fn handle(
        &mut self,
        msg: SeenInChainPayment<PendingPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(
            blocking::run({
                let pool = self.pool.clone();
                move || {
                    use crate::schema::transactions::dsl::*;
                    let conn: &PgConnection = &pool.get().unwrap();
                    Ok(
                        diesel::update(transactions.filter(id.eq(msg.payment.id.clone())))
                            .set((height.eq(msg.height), status.eq(TransactionStatus::InChain)))
                            .get_result(conn)
                            .map(|tx: Transaction| InChainPayment(tx))
                            .map_err::<Error, _>(|e| e.into())?,
                    )
                }
            })
            .from_err(),
        )
    }
}

impl Handler<SeenInChainPayment<RejectedPayment>> for Fsm {
    type Result = ResponseFuture<RefundPayment, Error>;

    fn handle(
        &mut self,
        msg: SeenInChainPayment<RejectedPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(
            blocking::run({
                let pool = self.pool.clone();
                move || {
                    use crate::schema::transactions::dsl::*;
                    let conn: &PgConnection = &pool.get().unwrap();
                    Ok(
                        diesel::update(transactions.filter(id.eq(msg.payment.id.clone())))
                            .set(status.eq(TransactionStatus::Refund))
                            .get_result(conn)
                            .map(|tx: Transaction| RefundPayment(tx))
                            .map_err::<Error, _>(|e| e.into())?,
                    )
                }
            })
            .from_err(),
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

impl Handler<GetUnreportedConfirmedPayments> for Fsm {
    type Result = ResponseFuture<Vec<ConfirmedPayment>, Error>;

    fn handle(&mut self, _: GetUnreportedConfirmedPayments, _: &mut Self::Context) -> Self::Result {
        Box::new(
            self.db
                .send(GetUnreportedPaymentsByStatus(TransactionStatus::Confirmed))
                .from_err()
                .and_then(|db_response| {
                    let data = db_response?;
                    Ok(data.into_iter().map(ConfirmedPayment).collect())
                }),
        )
    }
}

impl Handler<GetUnreportedRejectedPayments> for Fsm {
    type Result = ResponseFuture<Vec<RejectedPayment>, Error>;

    fn handle(&mut self, _: GetUnreportedRejectedPayments, _: &mut Self::Context) -> Self::Result {
        Box::new(
            self.db
                .send(GetUnreportedPaymentsByStatus(TransactionStatus::Rejected))
                .from_err()
                .and_then(|db_response| {
                    let data = db_response?;
                    Ok(data.into_iter().map(RejectedPayment).collect())
                }),
        )
    }
}

fn run_callback(
    callback_url: &str,
    token: &str,
    transaction: &Transaction,
) -> impl Future<Item = (), Error = Error> {
    client::post(callback_url)
        .json(Confirmation {
            id: &transaction.id,
            external_id: &transaction.external_id,
            merchant_id: &transaction.merchant_id,
            grin_amount: transaction.grin_amount,
            amount: &transaction.amount,
            status: transaction.status,
            confirmations: transaction.confirmations,
            token: token,
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
        Box::new(
            report_transaction(self.db.clone(), msg.payment.0.clone()).and_then({
                let pool = self.pool.clone();
                move |_| {
                    blocking::run({
                        move || {
                            let conn: &PgConnection = &pool.get().unwrap();
                            conn.transaction(|| {
                                {
                                    use crate::schema::merchants::dsl::*;
                                    diesel::update(
                                        merchants.filter(id.eq(msg.payment.merchant_id.clone())),
                                    )
                                    .set(balance.eq(balance + msg.payment.grin_amount))
                                    .get_result::<Merchant>(conn)
                                    .map_err::<Error, _>(|e| e.into())?;
                                };
                                use crate::schema::transactions::dsl::*;
                                diesel::update(transactions.filter(id.eq(msg.payment.id)))
                                    .set(reported.eq(true))
                                    .get_result::<Transaction>(conn)
                                    .map_err::<Error, _>(|e| e.into())?;
                                Ok(())
                            })
                        }
                    })
                    .from_err()
                }
            }),
        )
    }
}

impl Handler<ReportPayment<RejectedPayment>> for Fsm {
    type Result = ResponseFuture<(), Error>;

    fn handle(
        &mut self,
        msg: ReportPayment<RejectedPayment>,
        _: &mut Self::Context,
    ) -> Self::Result {
        Box::new(
            report_transaction(self.db.clone(), msg.payment.0.clone()).and_then({
                let pool = self.pool.clone();
                move |_| {
                    blocking::run({
                        move || {
                            let conn: &PgConnection = &pool.get().unwrap();
                            conn.transaction(|| {
                                {
                                    use crate::schema::merchants::dsl::*;
                                    diesel::update(
                                        merchants.filter(id.eq(msg.payment.merchant_id.clone())),
                                    )
                                    .set(balance.eq(balance + msg.payment.grin_amount))
                                    .get_result::<Merchant>(conn)
                                    .map_err::<Error, _>(|e| e.into())?;
                                };
                                use crate::schema::transactions::dsl::*;
                                diesel::update(transactions.filter(id.eq(msg.payment.id)))
                                    .set(reported.eq(true))
                                    .get_result::<Transaction>(conn)
                                    .map_err::<Error, _>(|e| e.into())?;

                                Ok(())
                            })
                        }
                    })
                    .from_err()
                }
            }),
        )
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
            let res = run_callback(&callback_url, &merchant.token, &transaction).or_else({
                let db = db.clone();
                let report_attempts = transaction.report_attempts.clone();
                let transaction_id = transaction.id.clone();
                move |callback_err| {
                    // try call ReportAttempt but ignore errors and return
                    // error from callback
                    let next_attempt = Utc::now().naive_utc()
                        + Duration::seconds(10 * (report_attempts + 1).pow(2) as i64);
                    db.send(ReportAttempt {
                        transaction_id: transaction_id,
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
            });
            Either::A(res)
        } else {
            Either::B(ok(()))
        }
    })
}
