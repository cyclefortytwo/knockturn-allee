use crate::app::AppState;
use crate::db::{CreateMerchant, GetMerchant};
use crate::errors::*;
use crate::extractor::SimpleJson;
use crate::models::{Transaction, TransactionStatus};
use actix_web::{AsyncResponder, FutureResponse, HttpResponse, Path, State};
use askama::Template;
use bcrypt;
use futures::future::{ok, result, Future};
use mime_guess::get_mime_type;

pub mod mfa;
pub mod payment;
pub mod webui;

pub fn create_merchant(
    (create_merchant, state): (SimpleJson<CreateMerchant>, State<AppState>),
) -> FutureResponse<HttpResponse> {
    let mut create_merchant = create_merchant.into_inner();
    create_merchant.password = match bcrypt::hash(&create_merchant.password, bcrypt::DEFAULT_COST) {
        Ok(v) => v,
        Err(_) => return result(Ok(HttpResponse::InternalServerError().finish())).responder(),
    };
    state
        .db
        .send(create_merchant)
        .from_err()
        .and_then(|db_response| {
            let merchant = db_response?;
            Ok(HttpResponse::Created().json(merchant))
        })
        .responder()
}

pub fn get_merchant(
    (merchant_id, state): (Path<String>, State<AppState>),
) -> FutureResponse<HttpResponse> {
    state
        .db
        .send(GetMerchant {
            id: merchant_id.to_owned(),
        })
        .from_err()
        .and_then(|db_response| {
            let merchant = db_response?;
            Ok(HttpResponse::Ok().json(merchant))
        })
        .responder()
}

pub trait TemplateIntoResponse {
    fn into_response(&self) -> Result<HttpResponse, Error>;
    fn into_future(&self) -> FutureResponse<HttpResponse, Error>;
}

impl<T: Template> TemplateIntoResponse for T {
    fn into_response(&self) -> Result<HttpResponse, Error> {
        let rsp = self.render().map_err(|e| Error::Template(s!(e)))?;
        let ctype = get_mime_type(T::extension().unwrap_or("txt")).to_string();
        Ok(HttpResponse::Ok().content_type(ctype.as_str()).body(rsp))
    }
    fn into_future(&self) -> FutureResponse<HttpResponse, Error> {
        Box::new(ok(self.into_response().into()))
    }
}

pub trait BootstrapColor {
    fn color(&self) -> &'static str;
}
impl BootstrapColor for Transaction {
    fn color(&self) -> &'static str {
        match self.status {
            TransactionStatus::Confirmed => "success",
            TransactionStatus::Pending => "info",
            TransactionStatus::Rejected => "secondary",
            _ => "light",
        }
    }
}
