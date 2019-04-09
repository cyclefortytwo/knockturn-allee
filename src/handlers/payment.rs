use crate::app::AppState;
use crate::db::{GetCurrentHeight, GetTransaction};
use crate::errors::*;
use crate::extractor::{BasicAuth, SimpleJson};
use crate::filters;
use crate::fsm::{CreatePayment, GetNewPayment, MakePayment};
use crate::handlers::BootstrapColor;
use crate::models::{Merchant, Money, Transaction, TransactionStatus};
use crate::qrcode;
use crate::wallet::Slate;
use actix_web::{AsyncResponder, FutureResponse, HttpResponse, Path, State};
use askama::Template;
use chrono_humanize::{Accuracy, HumanTime, Tense};
use data_encoding::BASE64;
use futures::future::ok;
use futures::future::Future;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Deserialize)]
pub struct CreatePaymentRequest {
    pub order_id: String,
    pub amount: Money,
    pub confirmations: i64,
    pub email: Option<String>,
    pub message: String,
    pub redirect_url: Option<String>,
}

pub fn create_payment(
    (merchant, merchant_id, payment_req, state): (
        BasicAuth<Merchant>,
        Path<String>,
        SimpleJson<CreatePaymentRequest>,
        State<AppState>,
    ),
) -> FutureResponse<HttpResponse> {
    let merchant_id = merchant_id.into_inner();
    if merchant.id != merchant_id {
        return Box::new(ok(HttpResponse::BadRequest().finish()));
    }
    let create_transaction = CreatePayment {
        merchant_id: merchant_id,
        external_id: payment_req.order_id.clone(),
        amount: payment_req.amount,
        confirmations: payment_req.confirmations,
        email: payment_req.email.clone(),
        message: payment_req.message.clone(),
        redirect_url: payment_req.redirect_url.clone(),
    };
    state
        .fsm
        .send(create_transaction)
        .from_err()
        .and_then(|db_response| {
            let new_payment = db_response?;

            Ok(HttpResponse::Created().json(new_payment))
        })
        .responder()
}

#[derive(Debug, Serialize)]
struct PaymentStatus {
    pub transaction_id: String,
    pub status: String,
    pub reported: bool,
    pub seconds_until_expired: Option<i64>,
    pub expired_in: Option<String>,
    pub current_confirmations: i64,
    pub required_confirmations: i64,
}

pub fn get_payment_status(
    (get_transaction, state): (Path<GetTransaction>, State<AppState>),
) -> FutureResponse<HttpResponse> {
    state
        .db
        .send(GetCurrentHeight)
        .from_err()
        .and_then(|db_response| {
            let height = db_response?;
            Ok(height)
        })
        .and_then({
            let db = state.db.clone();
            move |current_height| {
                db.send(get_transaction.into_inner())
                    .from_err()
                    .and_then(move |db_response| {
                        let tx = db_response?;
                        let payment_status = PaymentStatus {
                            transaction_id: tx.id.to_string(),
                            status: tx.status.to_string(),
                            seconds_until_expired: tx.time_until_expired().map(|d| d.num_seconds()),

                            expired_in: tx.time_until_expired().map(|d| {
                                HumanTime::from(d).to_text_en(Accuracy::Precise, Tense::Present)
                            }),
                            current_confirmations: tx.current_confirmations(current_height),
                            required_confirmations: tx.confirmations,
                            reported: tx.reported,
                        };
                        Ok(HttpResponse::Ok().json(payment_status))
                    })
            }
        })
        .responder()
}

pub fn get_payment(
    (get_transaction, state): (Path<GetTransaction>, State<AppState>),
) -> FutureResponse<HttpResponse> {
    state
        .db
        .send(GetCurrentHeight)
        .from_err()
        .and_then(|db_response| {
            let height = db_response?;
            Ok(height)
        })
        .and_then({
            let db = state.db.clone();
            move |current_height| {
                db.send(get_transaction.into_inner())
                    .from_err()
                    .and_then(move |db_response| {
                        let transaction = db_response?;

                        let payment_url = format!(
                            "{}/merchants/{}/payments/{}",
                            env::var("DOMAIN").unwrap().trim_end_matches('/'),
                            transaction.merchant_id,
                            transaction.id.to_string()
                        );
                        let ironbelly_link = format!(
                            "grin://send?amount={}&destination={}&message={}",
                            transaction.grin_amount,
                            payment_url,
                            BASE64.encode(transaction.message.as_bytes())
                        );
                        let html = PaymentTemplate {
                            payment: &transaction,
                            payment_url: payment_url,
                            current_height: current_height,
                            ironbelly_link: &ironbelly_link,
                            ironbelly_qrcode: &BASE64.encode(&qrcode::as_png(&ironbelly_link)?),
                        }
                        .render()
                        .map_err(|e| Error::from(e))?;
                        Ok(HttpResponse::Ok().content_type("text/html").body(html))
                    })
            }
        })
        .responder()
}

#[derive(Template)]
#[template(path = "payment.html")]
struct PaymentTemplate<'a> {
    payment: &'a Transaction,
    payment_url: String,
    current_height: i64,
    ironbelly_link: &'a str,
    ironbelly_qrcode: &'a str,
}

pub fn make_payment(
    (slate, payment, state): (SimpleJson<Slate>, Path<GetNewPayment>, State<AppState>),
) -> FutureResponse<HttpResponse, Error> {
    let slate_amount = slate.amount;
    state
        .fsm
        .send(payment.into_inner())
        .from_err()
        .and_then(move |db_response| {
            let new_payment = db_response?;
            let payment_amount = new_payment.grin_amount as u64;
            if new_payment.is_invalid_amount(slate_amount) {
                return Err(Error::WrongAmount(payment_amount, slate_amount));
            }
            Ok(new_payment)
        })
        .and_then({
            let wallet = state.wallet.clone();
            let fsm = state.fsm.clone();
            move |new_payment| {
                let slate = wallet.receive(&slate);
                slate.and_then(move |slate| {
                    let commit = slate.tx.output_commitments()[0].clone();
                    wallet
                        .get_tx(&slate.id.hyphenated().to_string())
                        .and_then(move |wallet_tx| {
                            fsm.send(MakePayment {
                                new_payment,
                                wallet_tx,
                                commit,
                            })
                            .from_err()
                            .and_then(|db_response| {
                                db_response?;
                                Ok(())
                            })
                        })
                        .and_then(|_| ok(slate))
                })
            }
        })
        .and_then(|slate| Ok(HttpResponse::Ok().json(slate)))
        .responder()
}
