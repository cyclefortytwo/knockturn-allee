use crate::clients::PlainHttpAuth;
use crate::errors::Error;
use crate::ser;
use actix::{Actor, Addr};
use actix_web::client::{self, ClientConnector};
use actix_web::HttpMessage;
use chrono::{DateTime, Utc};
use futures::Future;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use serde_json::from_slice;
use std::iter::Iterator;
use std::str::from_utf8;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct Wallet {
    conn: Addr<ClientConnector>,
    username: String,
    password: String,
    url: String,
}

const RETRIEVE_TXS_URL: &'static str = "v1/wallet/owner/retrieve_txs";
const RECEIVE_URL: &'static str = "v1/wallet/foreign/receive_tx";
const SEND_URL: &'static str = "/v1/wallet/owner/issue_send_tx";
const FINALIZE_URL: &'static str = "/v1/wallet/owner/finalize_tx";
const CANCEL_TX_URL: &'static str = "/v1/wallet/owner/cancel_tx";
const POST_TX_URL: &'static str = "/v1/wallet/owner/post_tx?fluff";

impl Wallet {
    pub fn new(url: &str, username: &str, password: &str) -> Self {
        let connector = ClientConnector::default()
            .conn_lifetime(Duration::from_secs(300))
            .conn_keep_alive(Duration::from_secs(300));
        Wallet {
            url: url.trim_end_matches('/').to_owned(),
            username: username.to_owned(),
            password: password.to_owned(),
            conn: connector.start(),
        }
    }

    pub fn get_tx(&self, tx_id: &str) -> impl Future<Item = TxLogEntry, Error = Error> {
        let tx_id = tx_id.to_owned();
        let url = format!("{}/{}?tx_id={}&refresh", self.url, RETRIEVE_TXS_URL, tx_id);
        debug!("Get transaction from wallet {}", url);
        client::get(&url) // <- Create request builder
            .auth(&self.username, &self.password)
            .finish()
            .unwrap()
            .send() // <- Send http request
            .map_err(|e| Error::WalletAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::WalletAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(resp)
                }
            })
            .and_then(|resp| {
                // <- server http response
                debug!("Response: {:?}", resp);
                resp.body()
                    .map_err(|e| Error::WalletAPIError(s!(e)))
                    .and_then(move |bytes| {
                        let txs: TxListResp = from_slice(&bytes).map_err(|e| {
                            error!(
                                "Cannot decode json {:?}:\n with error {} ",
                                from_utf8(&bytes),
                                e
                            );
                            Error::WalletAPIError(format!("Cannot decode json {}", e))
                        })?;
                        if txs.txs.len() == 0 {
                            return Err(Error::WalletAPIError(format!(
                                "Transaction with slate_id {} not found",
                                tx_id
                            )));
                        }
                        if txs.txs.len() > 1 {
                            return Err(Error::WalletAPIError(format!(
                                "Wallet returned more than one transaction with slate_id {}",
                                tx_id
                            )));
                        }
                        let tx = txs.txs.into_iter().next().unwrap();
                        Ok(tx)
                    })
            })
    }

    pub fn receive(&self, slate: &Slate) -> impl Future<Item = Slate, Error = Error> {
        let url = format!("{}/{}", self.url, RECEIVE_URL);
        debug!("Receive slate by wallet  {}", url);
        client::post(&url)
            .auth(&self.username, &self.password)
            .json(slate)
            .unwrap()
            .send()
            .map_err(|e| Error::WalletAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::WalletAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(resp)
                }
            })
            .and_then(|resp| {
                debug!("Response: {:?}", resp);
                resp.body()
                    .map_err(|e| Error::WalletAPIError(s!(e)))
                    .and_then(move |bytes| {
                        let slate_resp: Slate = from_slice(&bytes).map_err(|e| {
                            error!(
                                "Cannot decode json {:?}:\n with error {} ",
                                from_utf8(&bytes),
                                e
                            );
                            Error::WalletAPIError(format!("Cannot decode json {}", e))
                        })?;
                        Ok(slate_resp)
                    })
            })
    }

    pub fn finalize(&self, slate: &Slate) -> impl Future<Item = Slate, Error = Error> {
        let url = format!("{}/{}", self.url, FINALIZE_URL);
        debug!("Finalize slate by wallet {}", url);
        client::post(&url)
            .auth(&self.username, &self.password)
            .json(slate)
            .unwrap()
            .send()
            .map_err(|e| Error::WalletAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::WalletAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(resp)
                }
            })
            .and_then(|resp| {
                debug!("Response: {:?}", resp);
                resp.body()
                    .map_err(|e| Error::WalletAPIError(s!(e)))
                    .and_then(move |bytes| {
                        let slate_resp: Slate = from_slice(&bytes).map_err(|e| {
                            error!(
                                "Cannot decode json {:?}:\n with error {} ",
                                from_utf8(&bytes),
                                e
                            );
                            Error::WalletAPIError(format!("Cannot decode json {}", e))
                        })?;
                        Ok(slate_resp)
                    })
            })
    }
    pub fn cancel_tx(&self, tx_slate_id: &str) -> impl Future<Item = (), Error = Error> {
        let url = format!("{}/{}?tx_id={}", self.url, CANCEL_TX_URL, tx_slate_id);
        debug!("Cancel transaction in wallet {}", url);
        client::post(&url)
            .auth(&self.username, &self.password)
            .finish()
            .unwrap()
            .send()
            .map_err(|e| Error::WalletAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::WalletAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(())
                }
            })
    }

    pub fn post_tx(&self) -> impl Future<Item = (), Error = Error> {
        let url = format!("{}/{}", self.url, POST_TX_URL);
        debug!("Post transaction in chain by wallet as {}", url);
        client::post(&url)
            .auth(&self.username, &self.password)
            .finish()
            .unwrap()
            .send()
            .map_err(|e| Error::WalletAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::WalletAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(())
                }
            })
    }

    pub fn create_slate(
        &self,
        amount: u64,
        message: String,
    ) -> impl Future<Item = Slate, Error = Error> {
        let url = format!("{}/{}", self.url, SEND_URL);
        debug!("Receive as {} {}: {}", self.username, self.password, url);
        let payment = SendTx {
            amount: amount,
            minimum_confirmations: 10,
            method: "file",
            dest: "./gpp_always_pays.grinslate",
            max_outputs: 10,
            num_change_outputs: 1,
            selection_strategy_is_use_all: false,
            message: Some(message),
        };
        client::post(&url)
            .auth(&self.username, &self.password)
            .json(&payment)
            .unwrap()
            .send()
            .map_err(|e| Error::WalletAPIError(s!(e)))
            .and_then(|resp| {
                if !resp.status().is_success() {
                    Err(Error::WalletAPIError(format!("Error status: {:?}", resp)))
                } else {
                    Ok(resp)
                }
            })
            .and_then(|resp| {
                debug!("Response: {:?}", resp);
                resp.body()
                    .map_err(|e| Error::WalletAPIError(s!(e)))
                    .and_then(move |bytes| {
                        let slate_resp: Slate = from_slice(&bytes).map_err(|e| {
                            error!(
                                "Cannot decode json {:?}:\n with error {} ",
                                from_utf8(&bytes),
                                e
                            );
                            Error::WalletAPIError(format!("Cannot decode json {}", e))
                        })?;
                        Ok(slate_resp)
                    })
            })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxListResp {
    pub updated: bool,
    pub txs: Vec<TxLogEntry>,
}

/// Optional transaction information, recorded when an event happens
/// to add or remove funds from a wallet. One Transaction log entry
/// maps to one or many outputs
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxLogEntry {
    /// BIP32 account path used for creating this tx
    pub parent_key_id: Identifier,
    /// Local id for this transaction (distinct from a slate transaction id)
    pub id: u32,
    /// Slate transaction this entry is associated with, if any
    pub tx_slate_id: Option<String>,
    /// Transaction type (as above)
    pub tx_type: TxLogEntryType,
    /// Time this tx entry was created
    /// #[serde(with = "tx_date_format")]
    pub creation_ts: DateTime<Utc>,
    /// Time this tx was confirmed (by this wallet)
    /// #[serde(default, with = "opt_tx_date_format")]
    pub confirmation_ts: Option<DateTime<Utc>>,
    /// Whether the inputs+outputs involved in this transaction have been
    /// confirmed (In all cases either all outputs involved in a tx should be
    /// confirmed, or none should be; otherwise there's a deeper problem)
    pub confirmed: bool,
    /// number of inputs involved in TX
    pub num_inputs: usize,
    /// number of outputs involved in TX
    pub num_outputs: usize,
    /// Amount credited via this transaction
    #[serde(with = "ser::string_or_u64")]
    pub amount_credited: u64,
    /// Amount debited via this transaction
    #[serde(with = "ser::string_or_u64")]
    pub amount_debited: u64,
    /// Fee
    pub fee: Option<u64>,
    /// Message data, stored as json
    pub messages: Option<ParticipantMessages>,
    /// Location of the store transaction, (reference or resending)
    pub stored_tx: Option<String>,
}

pub type Identifier = String;

/*
#[derive(Clone, PartialEq, Eq, Ord, Hash, PartialOrd)]
pub struct Identifier([u8; IDENTIFIER_SIZE]);
*/

/// Types of transactions that can be contained within a TXLog entry
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum TxLogEntryType {
    /// A coinbase transaction becomes confirmed
    ConfirmedCoinbase,
    /// Outputs created when a transaction is received
    TxReceived,
    /// Inputs locked + change outputs when a transaction is created
    TxSent,
    /// Received transaction that was rolled back by user
    TxReceivedCancelled,
    /// Sent transaction that was rolled back by user
    TxSentCancelled,
}

/// Helper just to facilitate serialization
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParticipantMessages {
    /// included messages
    pub messages: Vec<ParticipantMessageData>,
}

/// Public message data (for serialising and storage)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParticipantMessageData {
    /// id of the particpant in the tx
    #[serde(with = "ser::string_or_u64")]
    pub id: u64,
    /// Public key
    pub public_key: String,
    /// Message,
    pub message: Option<String>,
    /// Signature
    pub message_sig: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParticipantData {
    /// Id of participant in the transaction. (For now, 0=sender, 1=rec)
    pub id: u64,
    /// Public key corresponding to private blinding factor
    pub public_blind_excess: Vec<u8>,
    /// Public key corresponding to private nonce
    pub public_nonce: Vec<u8>,
    /// Public partial signature
    pub part_sig: Option<Vec<u8>>,
    /// A message for other participants
    pub message: Option<String>,
    /// Signature, created with private key corresponding to 'public_blind_excess'
    pub message_sig: Option<Vec<u8>>,
}

/// A 'Slate' is passed around to all parties to build up all of the public
/// transaction data needed to create a finalized transaction. Callers can pass
/// the slate around by whatever means they choose, (but we can provide some
/// binary or JSON serialization helpers here).

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Slate {
    /// The number of participants intended to take part in this transaction
    pub num_participants: usize,
    /// Unique transaction ID, selected by sender
    pub id: Uuid,
    /// The core transaction data:
    /// inputs, outputs, kernels, kernel offset
    pub tx: Transaction,
    /// base amount (excluding fee)
    pub amount: u64,
    /// fee amount
    pub fee: u64,
    /// Block height for the transaction
    pub height: u64,
    /// Lock height
    pub lock_height: u64,
    /// Participant data, each participant in the transaction will
    /// insert their public data here. For now, 0 is sender and 1
    /// is receiver, though this will change for multi-party
    pub participant_data: Vec<ParticipantData>,
    /// Slate format version
    #[serde(default = "no_version")]
    pub version: u64,
}

fn no_version() -> u64 {
    0
}

/// A range proof. Typically much larger in memory that the above (~5k).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeProof {
    /// The proof itself, at most 5134 bytes long
    pub proof: Vec<u8>,
    /// The length of the proof
    pub plen: usize,
}

/// Output for a transaction, defining the new ownership of coins that are being
/// transferred. The commitment is a blinded value for the output while the
/// range proof guarantees the commitment includes a positive value without
/// overflow and the ownership of the private key. The switch commitment hash
/// provides future-proofing against quantum-based attacks, as well as providing
/// wallet implementations with a way to identify their outputs for wallet
/// reconstruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    /// Options for an output's structure or use
    pub features: OutputFeatures,
    /// The homomorphic commitment representing the output amount
    pub commit: Vec<u8>,
    /// A proof that the commitment is in the right range
    pub proof: Vec<u8>,
}

/// A transaction input.
///
/// Primarily a reference to an output being spent by the transaction.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Input {
    /// The features of the output being spent.
    /// We will check maturity for coinbase output.
    pub features: OutputFeatures,
    /// The commit referencing the output being spent.
    pub commit: Vec<u8>,
}

/// Enum of various supported kernel "features".
/// Various flavors of tx kernel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum KernelFeatures {
    /// Plain kernel (the default for Grin txs).
    Plain = 0,
    /// A coinbase kernel.
    Coinbase = 1,
    /// A kernel with an expicit lock height.
    HeightLocked = 2,
}

/// A proof that a transaction sums to zero. Includes both the transaction's
/// Pedersen commitment and the signature, that guarantees that the commitments
/// amount to zero.
/// The signature signs the fee and the lock_height, which are retained for
/// signature validation.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TxKernel {
    /// Options for a kernel's structure or use
    pub features: KernelFeatures,
    /// Fee originally included in the transaction this proof is for.
    pub fee: u64,
    /// This kernel is not valid earlier than lock_height blocks
    /// The max lock_height of all *inputs* to this transaction
    pub lock_height: u64,
    /// Remainder of the sum of all transaction commitments. If the transaction
    /// is well formed, amounts components should sum to zero and the excess
    /// is hence a valid public key.
    pub excess: Vec<u8>,
    /// The signature proving the excess is a valid public key, which signs
    /// the transaction fee.
    pub excess_sig: Vec<u8>,
}

/// TransactionBody is a common abstraction for transaction and block
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransactionBody {
    /// List of inputs spent by the transaction.
    pub inputs: Vec<Input>,
    /// List of outputs the transaction produces.
    pub outputs: Vec<Output>,
    /// List of kernels that make up this transaction (usually a single kernel).
    pub kernels: Vec<TxKernel>,
}

/// A transaction
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    /// The kernel "offset" k2
    /// excess is k1G after splitting the key k = k1 + k2
    pub offset: Vec<u8>,
    /// The transaction body - inputs/outputs/kernels
    body: TransactionBody,
}

impl Transaction {
    pub fn output_commitments(&self) -> Vec<Vec<u8>> {
        self.body.outputs.iter().map(|o| o.commit.clone()).collect()
    }
}

/// Enum of various supported kernel "features".
/// Various flavors of tx kernel.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[repr(u8)]
pub enum OutputFeatures {
    /// Plain output (the default for Grin txs).
    Plain = 0,
    /// A coinbase output.
    Coinbase = 1,
}

#[derive(Debug, Serialize)]
struct SendTx {
    amount: u64,
    minimum_confirmations: u64,
    method: &'static str,
    dest: &'static str,
    max_outputs: u8,
    num_change_outputs: u8,
    selection_strategy_is_use_all: bool,
    message: Option<String>,
}

#[cfg(test)]
mod tests {

    #[test]
    fn wallet_get_tx_test() {
        assert!(true);
    }
    #[test]
    fn txs_read_test() {}
}
