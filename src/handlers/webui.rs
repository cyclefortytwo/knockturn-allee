use crate::app::AppState;
use crate::blocking;
use crate::db::GetMerchant;
use crate::errors::*;
use crate::extractor::Identity;
use crate::filters;
use crate::handlers::BootstrapColor;
use crate::handlers::TemplateIntoResponse;
use crate::models::{Merchant, Transaction, TransactionType};
use actix_web::middleware::identity::RequestIdentity;
use actix_web::middleware::session::RequestSession;
use actix_web::{AsyncResponder, Form, FutureResponse, HttpRequest, HttpResponse};
use askama::Template;
use diesel::pg::PgConnection;
use diesel::{self, prelude::*};
use futures::future::Future;
use serde::Deserialize;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    merchant: &'a Merchant,
    transactions: Vec<Transaction>,
    current_height: i64,
}

pub fn index(
    (merchant, req): (Identity<Merchant>, HttpRequest<AppState>),
) -> FutureResponse<HttpResponse> {
    let merchant = merchant.into_inner();
    blocking::run({
        let merch_id = merchant.id.clone();
        let pool = req.state().pool.clone();
        move || {
            let conn: &PgConnection = &pool.get().unwrap();
            let txs = {
                use crate::schema::transactions::dsl::*;
                transactions
                    .filter(merchant_id.eq(merch_id.clone()))
                    .offset(0)
                    .limit(10)
                    .order(created_at.desc())
                    .load::<Transaction>(conn)
                    .map_err::<Error, _>(|e| e.into())
            }?;
                      let current_height = {
                use crate::schema::current_height::dsl::*;
                current_height
                    .select(height)
                    .first(conn)
                    .map_err::<Error, _>(|e| e.into())
            }?;
            Ok((txs, current_height))
        }
    })
    .from_err()
    .and_then(move |(transactions,  current_height)| {
        let html = IndexTemplate {
            merchant: &merchant,
            transactions: transactions,
            current_height: current_height,
        }
        .render()
        .map_err(|e| Error::from(e))?;
        Ok(HttpResponse::Ok().content_type("text/html").body(html))
    })
    .responder()
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub login: String,
    pub password: String,
}
pub fn login(
    (req, login_form): (HttpRequest<AppState>, Form<LoginRequest>),
) -> FutureResponse<HttpResponse> {
    req.state()
        .db
        .send(GetMerchant {
            id: login_form.login.clone(),
        })
        .from_err()
        .and_then(move |db_response| {
            let merchant = db_response?;
            match bcrypt::verify(&login_form.password, &merchant.password) {
                Ok(res) => {
                    if res {
                        req.session().set("merchant", merchant.id)?;
                        if merchant.confirmed_2fa {
                            Ok(HttpResponse::Found().header("location", "/2fa").finish())
                        } else {
                            Ok(HttpResponse::Found()
                                .header("location", "/set_2fa")
                                .finish())
                        }
                    } else {
                        Ok(HttpResponse::Found().header("location", "/login").finish())
                    }
                }
                Err(_) => Ok(HttpResponse::Found().header("location", "/login").finish()),
            }
        })
        .responder()
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate;

pub fn login_form(_: HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    LoginTemplate.into_response()
}

pub fn logout(req: HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    req.forget();
    req.session().clear();
    Ok(HttpResponse::Found().header("location", "/login").finish())
}

#[derive(Template)]
#[template(path = "transactions.html")]
struct TransactionsTemplate {
    transactions: Vec<Transaction>,
    current_height: i64,
}

pub fn get_transactions(
    (merchant, req): (Identity<Merchant>, HttpRequest<AppState>),
) -> FutureResponse<HttpResponse> {
    let merchant = merchant.into_inner();
    blocking::run({
        let merch_id = merchant.id.clone();
        let pool = req.state().pool.clone();
        move || {
            use crate::schema::transactions::dsl::*;
            let conn: &PgConnection = &pool.get().unwrap();
            let txs = transactions
                .filter(merchant_id.eq(merch_id))
                .offset(0)
                .limit(10)
                .load::<Transaction>(conn)
                .map_err::<Error, _>(|e| e.into())?;

            let current_height = {
                use crate::schema::current_height::dsl::*;
                current_height
                    .select(height)
                    .first(conn)
                    .map_err::<Error, _>(|e| e.into())
            }?;
            Ok((txs, current_height))
        }
    })
    .from_err()
    .and_then(|(transactions, current_height)| {
        let html = TransactionsTemplate {
            transactions,
            current_height,
        }
        .render()
        .map_err(|e| Error::from(e))?;
        Ok(HttpResponse::Ok().content_type("text/html").body(html))
    })
    .responder()
}
