use crate::db::{DbExecutor, RegisterRate};
use actix::prelude::*;
use actix_web::client;
use actix_web::HttpMessage;
use futures;
use futures::future::{err, ok, result, Future};
use log::*;
use serde::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::str;

#[derive(Debug, Deserialize)]
struct Rates {
    grin: HashMap<String, f64>,
}

pub struct RatesFetcher {
    db: Addr<DbExecutor>,
}

impl RatesFetcher {
    pub fn new(db: Addr<DbExecutor>) -> Self {
        RatesFetcher { db }
    }

    pub fn fetch(&self) {
        let db = self.db.clone();
        let f = client::get(
            "https://api.coingecko.com/api/v3/simple/price?ids=grin&vs_currencies=btc%2Cusd%2Ceur",
        )
        .header("Accept", "application/json")
        .finish()
        .unwrap()
        .send()
        .map_err(|e| {
            error!("failed to fetch exchange rates: {:?}", e);
            ()
        })
        .and_then(|response| {
            response
                .body()
                .map_err(|e| {
                    error!("Payload error: {:?}", e);
                    ()
                })
                .and_then(move |body| match str::from_utf8(&body) {
                    Ok(v) => ok(v.to_owned()),
                    Err(e) => {
                        error!("failed to parse body: {:?}", e);
                        err(())
                    }
                })
                .and_then(|str| {
                    result(serde_json::from_str::<Rates>(&str).map_err(|e| {
                        error!("failed to parse json: {:?}", e);
                        ()
                    }))
                })
                .and_then(move |rates| {
                    db.send(RegisterRate { rates: rates.grin })
                        .map_err(|e| {
                            error!("failed to parse body: {:?}", e);
                            ()
                        })
                        .and_then(|db_response| match db_response {
                            Err(e) => {
                                error!("db error: {:?}", e);
                                err(())
                            }
                            Ok(_) => ok(()),
                        })
                })
        });
        actix::spawn(f);
    }
}
