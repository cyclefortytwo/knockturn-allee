use crate::blocking::BlockingError;
use actix::MailboxError;
use actix_web::{error::ResponseError, HttpResponse};
use failure::Fail;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "DB Error: {}", _0)]
    Db(String),

    #[fail(display = "Entity not found: {}", _0)]
    EntityNotFound(String),

    #[fail(display = "Invalid entity {}", _0)]
    InvalidEntity(String),

    #[fail(display = "Entity already exists {}", _0)]
    AlreadyExists(String),

    #[fail(display = "Template erorr")]
    Template(String),

    #[fail(display = "Unsupported currency: {}", _0)]
    UnsupportedCurrency(String),

    #[fail(display = "General error: {}", _0)]
    General(String),

    #[fail(display = "Got error when call wallet API {}", _0)]
    WalletAPIError(String),

    #[fail(display = "Got error when call Node API {}", _0)]
    NodeAPIError(String),

    #[fail(display = "Wrong amount. Required {} received {}", _0, _1)]
    WrongAmount(u64, u64),

    #[fail(display = "Wrong transaction status {}", _0)]
    WrongTransactionStatus(String),

    #[fail(display = "Cannot call callback_url {} : {}", callback_url, error)]
    MerchantCallbackError { callback_url: String, error: String },

    #[fail(display = "Internal error {}", _0)]
    Internal(String),

    #[fail(display = "Auth required")]
    AuthRequired,

    #[fail(display = "Not authorized")]
    NotAuthorized,

    #[fail(display = "Not authorized")]
    NotAuthorizedInUI,

    #[fail(display = "Merchant not found")]
    MerchantNotFound,

    #[fail(display = "Not enough funds")]
    NotEnoughFunds,
}

impl From<MailboxError> for Error {
    fn from(error: MailboxError) -> Self {
        Error::General(s!(error))
    }
}

impl From<BlockingError> for Error {
    fn from(error: BlockingError) -> Self {
        match error {
            BlockingError::Canceled => Error::General(s!("Got blocking error")),
            BlockingError::Error(e) => e,
        }
    }
}

impl From<diesel::result::Error> for Error {
    fn from(error: diesel::result::Error) -> Self {
        match error {
            diesel::result::Error::NotFound => Error::EntityNotFound(format!("Not found")),
            diesel::result::Error::DatabaseError(kind, _) => match kind {
                diesel::result::DatabaseErrorKind::UniqueViolation
                | diesel::result::DatabaseErrorKind::ForeignKeyViolation => {
                    Error::AlreadyExists("Already exists".to_owned())
                }
                _ => Error::Db(format!("{:?}", error)),
            },
            _ => Error::Db(format!("{:?}", error)),
        }
    }
}

impl From<askama::Error> for Error {
    fn from(error: askama::Error) -> Self {
        Error::Template(format!("{:?}", error))
    }
}

impl From<serde_json::error::Error> for Error {
    fn from(error: serde_json::error::Error) -> Self {
        Error::General(format!("{:?}", error))
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(error: std::str::Utf8Error) -> Self {
        Error::General(format!("{:?}", error))
    }
}

// impl ResponseError trait allows to convert our errors into http responses with appropriate data
impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        match *self {
            Error::Db(ref message) | Error::Template(ref message) => {
                HttpResponse::InternalServerError().json(message)
            }
            Error::EntityNotFound(ref message) => HttpResponse::NotFound().json(message),
            Error::InvalidEntity(ref message)
            | Error::AlreadyExists(ref message)
            | Error::UnsupportedCurrency(ref message) => HttpResponse::BadRequest().json(message),
            Error::AuthRequired => HttpResponse::Unauthorized().finish(),
            Error::NotAuthorized => HttpResponse::Forbidden().finish(),
            Error::NotAuthorizedInUI => HttpResponse::Found().header("location", "/login").finish(),
            _ => HttpResponse::InternalServerError().json("general error".to_owned()),
        }
    }
}
