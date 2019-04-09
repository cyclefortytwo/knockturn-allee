use crate::app::AppState;
use crate::db::GetMerchant;
use crate::errors::*;
use crate::models::Merchant;
use actix_web::middleware::identity::RequestIdentity;
use actix_web::middleware::session::RequestSession;
use actix_web::{FromRequest, HttpMessage, HttpRequest};
use actix_web_httpauth::extractors::basic;
use bytes::BytesMut;
use derive_deref::Deref;
use futures::future::{err, ok, Future};
use futures::stream::Stream;
use serde::de::DeserializeOwned;
use std::default::Default;

#[derive(Debug, Deref, Clone)]
pub struct BasicAuth<T>(pub T);

pub struct BasicAuthConfig(pub basic::Config);
impl Default for BasicAuthConfig {
    fn default() -> Self {
        let mut config = basic::Config::default();
        config.realm("knocktrun");
        BasicAuthConfig(config)
    }
}

impl FromRequest<AppState> for BasicAuth<Merchant> {
    type Config = BasicAuthConfig;
    type Result = Result<Box<dyn Future<Item = Self, Error = Error>>, Error>;

    fn from_request(req: &HttpRequest<AppState>, cfg: &Self::Config) -> Self::Result {
        let bauth =
            basic::BasicAuth::from_request(&req, &cfg.0).map_err(|_| Error::NotAuthorized)?;
        let username = bauth.username().to_owned();

        Ok(Box::new(
            req.state()
                .db
                .send(GetMerchant { id: username })
                .from_err()
                .and_then(move |db_response| {
                    let merchant = match db_response {
                        Ok(m) => m,
                        Err(_) => return err(Error::NotAuthorized),
                    };
                    let password = bauth.password().unwrap_or("");
                    if merchant.token != password {
                        err(Error::NotAuthorized)
                    } else {
                        ok(BasicAuth(merchant))
                    }
                }),
        ))
    }
}

/// Session extractor
#[derive(Debug, Deref, Clone)]
pub struct Session<T>(pub T);

impl<T> Session<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

pub struct SessionConfig(String);

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig("merchant".to_owned())
    }
}

impl FromRequest<AppState> for Session<Merchant> {
    type Config = SessionConfig;
    type Result = Result<Box<dyn Future<Item = Self, Error = Error>>, Error>;

    fn from_request(req: &HttpRequest<AppState>, cfg: &Self::Config) -> Self::Result {
        let merchant_id = match req.session().get::<String>(&cfg.0) {
            Ok(Some(v)) => v,
            _ => return Err(Error::NotAuthorizedInUI),
        };

        Ok(Box::new(
            req.state()
                .db
                .send(GetMerchant { id: merchant_id })
                .from_err()
                .and_then(move |db_response| match db_response {
                    Ok(m) => ok(Session(m)),
                    Err(_) => err(Error::NotAuthorizedInUI),
                }),
        ))
    }
}

/// Identity extractor
#[derive(Debug, Deref, Clone)]
pub struct Identity<T>(pub T);

impl<T> Identity<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

pub struct IdentityConfig;

impl Default for IdentityConfig {
    fn default() -> Self {
        IdentityConfig {}
    }
}

impl FromRequest<AppState> for Identity<Merchant> {
    type Config = IdentityConfig;
    type Result = Result<Box<dyn Future<Item = Self, Error = Error>>, Error>;

    fn from_request(req: &HttpRequest<AppState>, _: &Self::Config) -> Self::Result {
        let merchant_id = match req.identity() {
            Some(v) => v,
            None => return Err(Error::NotAuthorizedInUI),
        };

        Ok(Box::new(
            req.state()
                .db
                .send(GetMerchant { id: merchant_id })
                .from_err()
                .and_then(move |db_response| match db_response {
                    Ok(m) => ok(Identity(m)),
                    Err(_) => err(Error::NotAuthorizedInUI),
                }),
        ))
    }
}

/// Json extractor
#[derive(Debug, Deref, Clone)]
pub struct SimpleJson<T>(pub T);

impl<T> SimpleJson<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

pub struct SimpleJsonConfig;

impl Default for SimpleJsonConfig {
    fn default() -> Self {
        SimpleJsonConfig {}
    }
}
const MAX_SIZE: usize = 262_144 * 1024; // max payload size is 256m

impl<T> FromRequest<AppState> for SimpleJson<T>
where
    T: DeserializeOwned + 'static,
{
    type Config = SimpleJsonConfig;
    type Result = Result<Box<dyn Future<Item = Self, Error = Error>>, Error>;

    fn from_request(req: &HttpRequest<AppState>, _cfg: &Self::Config) -> Self::Result {
        Ok(Box::new(
            req.payload()
                .map_err(|e| Error::Internal(format!("Payload error: {:?}", e)))
                .fold(BytesMut::new(), move |mut body, chunk| {
                    if (body.len() + chunk.len()) > MAX_SIZE {
                        Err(Error::Internal("overflow".to_owned()))
                    } else {
                        body.extend_from_slice(&chunk);
                        Ok(body)
                    }
                })
                .and_then(|body| {
                    let obj = serde_json::from_slice::<T>(&body)?;
                    Ok(SimpleJson(obj))
                }),
        ))
    }
}
