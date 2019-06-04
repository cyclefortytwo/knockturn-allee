use crate::blocking;
use crate::db::{DbExecutor, RejectExpiredPayments};
use crate::errors::Error;
use crate::fsm::{
    Fsm, GetPendingPayments, GetUnreportedConfirmedPayments, GetUnreportedRejectedPayments,
    RejectPayment, ReportPayment,
};
use crate::models::{Transaction, TransactionStatus};
use crate::node::Node;
use crate::rates::RatesFetcher;
use actix::prelude::*;
use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::{self, prelude::*};
use futures::future::{join_all, Future};
use log::*;
use std::collections::HashMap;

const REQUST_BLOCKS_FROM_NODE: i64 = 10;

pub struct Cron {
    db: Addr<DbExecutor>,
    node: Node,
    fsm: Addr<Fsm>,
    pool: Pool<ConnectionManager<PgConnection>>,
}

impl Actor for Cron {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        info!("Starting cron process");
        let rates = RatesFetcher::new(self.db.clone());
        ctx.run_interval(
            std::time::Duration::new(5, 0),
            move |_instance: &mut Cron, _ctx: &mut Context<Self>| {
                rates.fetch();
            },
        );
        ctx.run_interval(std::time::Duration::new(5, 0), reject_expired_payments);
        ctx.run_interval(std::time::Duration::new(5, 0), process_pending_payments);
        ctx.run_interval(
            std::time::Duration::new(5, 0),
            process_unreported_confirmed_payments,
        );
        ctx.run_interval(
            std::time::Duration::new(5, 0),
            process_unreported_rejected_payments,
        );
        ctx.run_interval(std::time::Duration::new(5, 0), sync_with_node);
        ctx.run_interval(std::time::Duration::new(5, 0), autoconfirmation);
    }

    fn stopping(&mut self, _ctx: &mut Self::Context) -> Running {
        Running::Stop
    }
}

impl Cron {
    pub fn new(
        db: Addr<DbExecutor>,
        fsm: Addr<Fsm>,
        node: Node,
        pool: Pool<ConnectionManager<PgConnection>>,
    ) -> Self {
        Cron {
            db,
            fsm,
            node,
            pool,
        }
    }
}
fn reject_expired_payments(cron: &mut Cron, _: &mut Context<Cron>) {
    debug!("run process_expired_payments");
    let res = cron
        .db
        .send(RejectExpiredPayments)
        .map_err(|e| Error::from(e))
        .and_then(|db_response| {
            db_response?;
            Ok(())
        });
    actix::spawn(res.map_err(|e| error!("Got an error in rejecting exprired payments {}", e)));
}

fn process_pending_payments(cron: &mut Cron, _: &mut Context<Cron>) {
    debug!("run process_pending_payments");
    let fsm = cron.fsm.clone();
    let res = cron
        .fsm
        .send(GetPendingPayments)
        .map_err(|e| Error::General(s!(e)))
        .and_then(move |db_response| {
            let payments = db_response?;
            Ok(payments)
        })
        .and_then(move |payments| {
            let mut futures = vec![];
            debug!("Found {} pending payments", payments.len());
            for payment in payments {
                if payment.is_expired() {
                    debug!("payment {} expired: try to reject it", payment.id);
                    futures.push(
                        fsm.send(RejectPayment {
                            payment: payment.clone(),
                        })
                        .map_err(|e| Error::General(s!(e)))
                        .and_then(|db_response| {
                            db_response?;
                            Ok(())
                        })
                        .or_else(move |e| {
                            error!("Cannot reject payment {}: {}", payment.id, e);
                            Ok(())
                        }),
                    );
                }
            }
            join_all(futures).map(|_| ())
        });
    actix::spawn(res.map_err(|e| error!("Got an error in processing penging payments {}", e)));
}

fn process_unreported_confirmed_payments(cron: &mut Cron, _: &mut Context<Cron>) {
    let res = cron
        .fsm
        .send(GetUnreportedConfirmedPayments)
        .map_err(|e| Error::General(s!(e)))
        .and_then(move |db_response| {
            let payments = db_response?;
            Ok(payments)
        })
        .and_then({
            let fsm = cron.fsm.clone();
            move |payments| {
                let mut futures = vec![];
                debug!("Found {} unreported payments", payments.len());
                for payment in payments {
                    let payment_id = payment.id.clone();
                    futures.push(
                        fsm.send(ReportPayment { payment })
                            .map_err(|e| Error::General(s!(e)))
                            .and_then(|db_response| {
                                db_response?;
                                Ok(())
                            })
                            .or_else({
                                move |e| {
                                    warn!("Couldn't report payment {}: {}", payment_id, e);
                                    Ok(())
                                }
                            }),
                    );
                }
                join_all(futures).map(|_| ()).map_err(|e| {
                    error!("got an error {}", e);
                    e
                })
            }
        });

    actix::spawn(res.map_err(|e| {
        error!("got an error {}", e);
        ()
    }));
}

fn process_unreported_rejected_payments(cron: &mut Cron, _: &mut Context<Cron>) {
    let res = cron
        .fsm
        .send(GetUnreportedRejectedPayments)
        .map_err(|e| Error::General(s!(e)))
        .and_then(move |db_response| {
            let payments = db_response?;
            Ok(payments)
        })
        .and_then({
            let fsm = cron.fsm.clone();
            move |payments| {
                let mut futures = vec![];
                debug!("Found {} unreported payments", payments.len());
                for payment in payments {
                    let payment_id = payment.id.clone();
                    futures.push(
                        fsm.send(ReportPayment { payment })
                            .map_err(|e| Error::General(s!(e)))
                            .and_then(|db_response| {
                                db_response?;
                                Ok(())
                            })
                            .or_else({
                                move |e| {
                                    warn!("Couldn't report payment {}: {}", payment_id, e);
                                    Ok(())
                                }
                            }),
                    );
                }
                join_all(futures).map(|_| ()).map_err(|e| {
                    error!("got an error {}", e);
                    e
                })
            }
        });

    actix::spawn(res.map_err(|e| {
        error!("got an error {}", e);
        ()
    }));
}
fn sync_with_node(cron: &mut Cron, _: &mut Context<Cron>) {
    debug!("run sync_with_node");
    let pool = cron.pool.clone();
    let node = cron.node.clone();
    let res = blocking::run({
        let pool = pool.clone();
        move || {
            use crate::schema::current_height::dsl::*;
            let conn: &PgConnection = &pool.get().unwrap();
            let last_height: i64 = current_height.select(height).first(conn)?;
            Ok(last_height)
        }
    })
    .map_err(|e| e.into())
    .and_then(move |last_height| {
        node.blocks(last_height + 1, last_height + 1 + REQUST_BLOCKS_FROM_NODE)
            .and_then(move |blocks| {
                let new_height = blocks
                    .iter()
                    .fold(last_height as u64, |current_height, block| {
                        if block.header.height > current_height {
                            block.header.height
                        } else {
                            current_height
                        }
                    });
                let commits: HashMap<String, i64> = blocks
                    .iter()
                    .flat_map(|block| block.outputs.iter())
                    .filter(|o| !o.is_coinbase())
                    .filter(|o| o.block_height.is_some())
                    .map(|o| (o.commit.clone(), o.block_height.unwrap() as i64))
                    .collect();
                debug!("Found {} non coinbase outputs", commits.len());
                blocking::run({
                    let pool = pool.clone();
                    move || {
                        use crate::schema::transactions::dsl::*;
                        let conn: &PgConnection = &pool.get().unwrap();
                        conn.transaction(move || {
                            let txs = transactions
                                .filter(commit.eq_any(commits.keys()))
                                .load::<Transaction>(conn)?;

                            if txs.len() > 0 {
                                debug!("Found {} transactions which got into chain", txs.len());
                            }
                            for tx in txs {
                                let query =
                                    diesel::update(transactions.filter(id.eq(tx.id.clone())));

                                match tx.status {
                                    TransactionStatus::Pending => query.set((
                                        status.eq(TransactionStatus::InChain),
                                        height.eq(commits.get(&tx.commit.unwrap()).unwrap()),
                                    )),
                                    TransactionStatus::Rejected => query.set((
                                        status.eq(TransactionStatus::Refund),
                                        height.eq(commits.get(&tx.commit.unwrap()).unwrap()),
                                    )),
                                    _ => {
                                        return Err(Error::General(format!(
                                            "Transaction {} in chain although it has status {}",
                                            tx.id.clone(),
                                            tx.status
                                        )))
                                    }
                                }
                                .get_result(conn)
                                .map(|_: Transaction| ())
                                .map_err::<Error, _>(|e| e.into())?;
                            }
                            {
                                debug!("Set new last_height = {}", new_height);
                                use crate::schema::current_height::dsl::*;
                                diesel::update(current_height)
                                    .set(height.eq(new_height as i64))
                                    .execute(conn)
                                    .map(|_| ())
                                    .map_err::<Error, _>(|e| e.into())?;
                            }
                            Ok(())
                        })
                    }
                })
                .from_err()
            })
    });
    actix::spawn(res.map_err(|e: Error| error!("Got an error trying to sync with node: {}", e)));
}

fn autoconfirmation(cron: &mut Cron, _: &mut Context<Cron>) {
    debug!("run autoconfirmation");
    let res = blocking::run({
        let pool = cron.pool.clone();
        move || {
            let conn: &PgConnection = &pool.get().unwrap();
            let last_height = {
                use crate::schema::current_height::dsl::*;
                let last_height: i64 = current_height.select(height).first(conn)?;
                last_height
            };

            use diesel::sql_query;
            sql_query(format!(
                "UPDATE transactions SET status = 'confirmed' WHERE
            status = 'in_chain' and confirmations < {} - height",
                last_height
            ))
            .execute(conn)?;
            //use crate::schema::transactions::dsl::*;
            //diesel::update(
            //transactions
            //.filter(status.eq(TransactionStatus::InChain))
            //.filter(confirmations.lt(last_height - height)),
            //)
            //.set(status.eq(TransactionStatus::Confirmed))
            //.execute(conn)?;

            Ok(())
        }
    })
    .from_err();
    actix::spawn(res.map_err(|e: Error| error!("Got an error trying to sync with node: {}", e)));
}
