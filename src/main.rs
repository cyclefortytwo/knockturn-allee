#[macro_use]
mod macros;

mod app;
mod blocking;
mod clients;
mod cron;
mod db;
mod errors;
mod extractor;
mod filters;
mod fsm;
mod handlers;
mod models;
mod node;
mod qrcode;
mod rates;
mod schema;
mod totp;
mod wallet;

#[macro_use]
extern crate diesel;

use crate::db::DbExecutor;
use crate::fsm::Fsm;
use crate::node::Node;
use crate::wallet::Wallet;
use actix::prelude::*;
use actix_web::server;
use diesel::{r2d2::ConnectionManager, PgConnection};
use dotenv::dotenv;
use env_logger;
use log::info;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use std::env;

fn main() {
    dotenv().ok();

    env_logger::init();

    let cookie_secret = env::var("COOKIE_SECRET").expect("COOKIE_SECRET must be set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let host = env::var("HOST").unwrap_or("0.0.0.0:3000".to_owned());
    let _ = env::var("DOMAIN").expect("DOMAIN must be set");
    let sys = actix::System::new("Knockout");

    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");

    let pool_clone = pool.clone();
    let address: Addr<DbExecutor> = SyncArbiter::start(10, move || DbExecutor(pool_clone.clone()));

    let wallet_url = env::var("WALLET_URL").expect("WALLET_URL must be set");
    let wallet_user = env::var("WALLET_USER").expect("WALLET_USER must be set");
    let wallet_pass = env::var("WALLET_PASS").expect("WALLET_PASS must be set");

    let wallet = Wallet::new(&wallet_url, &wallet_user, &wallet_pass);

    let node_url = env::var("NODE_URL").expect("NODE_URL must be set");
    let node_user = env::var("NODE_USER").expect("NODE_USER must be set");
    let node_pass = env::var("NODE_PASS").expect("NODE_PASS must be set");
    let node = Node::new(&node_url, &node_user, &node_pass);

    info!("Starting");
    let cron_db = address.clone();

    let fsm: Addr<Fsm> = Arbiter::start({
        let wallet = wallet.clone();
        let db = address.clone();
        let pool = pool.clone();
        move |_| Fsm { db, wallet, pool }
    });
    let _cron = Arbiter::start({
        let fsm = fsm.clone();
        let pool = pool.clone();
        move |_| cron::Cron::new(cron_db, fsm, node, pool)
    });

    let mut srv = server::new(move || {
        app::create_app(
            address.clone(),
            wallet.clone(),
            fsm.clone(),
            pool.clone(),
            cookie_secret.as_bytes(),
        )
    });

    srv = if let Ok(folder) = env::var("TLS_FOLDER") {
        let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
        builder
            .set_private_key_file(format!("{}/privkey.pem", folder), SslFiletype::PEM)
            .unwrap();
        builder
            .set_certificate_chain_file(format!("{}/fullchain.pem", folder))
            .unwrap();
        srv.bind_ssl(&host, builder)
            .expect(&format!("Can not bind_ssl to '{}'", &host))
    } else {
        srv.bind(&host)
            .expect(&format!("Can not bind to '{}'", &host))
    };
    srv.start();
    sys.run();
}
