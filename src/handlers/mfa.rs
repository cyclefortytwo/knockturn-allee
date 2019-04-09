use crate::app::AppState;
use crate::db::{Confirm2FA, GetMerchant};
use crate::errors::*;
use crate::extractor::Session;
use crate::handlers::TemplateIntoResponse;
use crate::models::Merchant;
use crate::totp::Totp;
use actix_web::http::Method;
use actix_web::middleware::identity::RequestIdentity;
use actix_web::middleware::session::RequestSession;
use actix_web::{AsyncResponder, Form, FutureResponse, HttpRequest, HttpResponse};
use askama::Template;
use data_encoding::BASE64;
use futures::future::Future;
use futures::future::{err, ok};
use serde::Deserialize;

#[derive(Template)]
#[template(path = "totp.html")]
struct TotpTemplate<'a> {
    msg: &'a str,
    token: &'a str,
    image: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct TotpRequest {
    pub code: String,
}

#[derive(Template)]
#[template(path = "2fa.html")]
struct TwoFATemplate;

pub fn form_2fa(_: HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    TwoFATemplate {}.into_response()
}

pub fn get_totp(merchant: Session<Merchant>) -> Result<HttpResponse, Error> {
    let merchant = merchant.into_inner();
    let token = merchant
        .token_2fa
        .ok_or(Error::General(s!("No 2fa token")))?;
    let totp = Totp::new(merchant.id.clone(), token.clone());

    let html = TotpTemplate {
        msg: "",
        token: &token,
        image: &BASE64.encode(&totp.get_png()?),
    }
    .render()
    .map_err(|e| Error::from(e))?;
    Ok(HttpResponse::Ok().content_type("text/html").body(html))
}

pub fn post_totp(
    (merchant, req, totp_form): (Session<Merchant>, HttpRequest<AppState>, Form<TotpRequest>),
) -> FutureResponse<HttpResponse, Error> {
    let merchant = merchant.into_inner();
    let mut msg = String::new();

    let token = match merchant.token_2fa {
        Some(t) => t,
        None => return Box::new(err(Error::General(s!("No 2fa token")))),
    };
    let totp = Totp::new(merchant.id.clone(), token.clone());

    if req.method() == Method::POST {
        match totp.check(&totp_form.code) {
            Ok(true) => {
                let resp = HttpResponse::Found().header("location", "/").finish();
                return req
                    .state()
                    .db
                    .send(Confirm2FA {
                        merchant_id: merchant.id,
                    })
                    .from_err()
                    .and_then(move |db_response| {
                        db_response?;
                        Ok(resp)
                    })
                    .responder();
            }
            _ => msg.push_str("Incorrect code, please try one more time"),
        }
    }

    let image = match totp.get_png() {
        Err(_) => return Box::new(err(Error::General(s!("can't generate an image")))),
        Ok(v) => v,
    };

    let html = match (TotpTemplate {
        msg: &msg,
        token: &token,
        image: &BASE64.encode(&image),
    }
    .render())
    {
        Err(e) => return Box::new(err(Error::from(e))),
        Ok(v) => v,
    };
    let resp = HttpResponse::Ok().content_type("text/html").body(html);
    ok(resp).responder()
}

pub fn post_2fa(
    (req, totp_form): (HttpRequest<AppState>, Form<TotpRequest>),
) -> FutureResponse<HttpResponse, Error> {
    let merchant_id = match req.session().get::<String>("merchant") {
        Ok(Some(v)) => v,
        _ => {
            return Box::new(ok(HttpResponse::Found()
                .header("location", "/login")
                .finish()));
        }
    };
    req.state()
        .db
        .send(GetMerchant {
            id: merchant_id.clone(),
        })
        .from_err()
        .and_then(move |db_response| {
            let merchant = db_response?;

            let token = merchant
                .token_2fa
                .ok_or(Error::General(s!("No 2fa token")))?;
            let totp = Totp::new(merchant.id.clone(), token.clone());

            if totp.check(&totp_form.code)? {
                req.remember(merchant.id);
                return Ok(HttpResponse::Found().header("location", "/").finish());
            } else {
                Ok(HttpResponse::Found().header("location", "/2fa").finish())
            }
        })
        .responder()
}
