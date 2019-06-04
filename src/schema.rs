table! {
    use diesel::sql_types::*;
    use crate::models::Transaction_status;
    use crate::models::Transaction_type;

    current_height (height) {
        height -> Int8,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transaction_status;
    use crate::models::Transaction_type;

    merchants (id) {
        id -> Text,
        email -> Varchar,
        password -> Varchar,
        wallet_url -> Nullable<Text>,
        balance -> Int8,
        created_at -> Timestamp,
        token -> Text,
        callback_url -> Nullable<Text>,
        token_2fa -> Nullable<Varchar>,
        confirmed_2fa -> Bool,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transaction_status;
    use crate::models::Transaction_type;

    rates (id) {
        id -> Text,
        rate -> Float8,
        updated_at -> Timestamp,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transaction_status;
    use crate::models::Transaction_type;

    transactions (id) {
        id -> Uuid,
        external_id -> Text,
        merchant_id -> Text,
        grin_amount -> Int8,
        amount -> Jsonb,
        status -> Transaction_status,
        confirmations -> Int8,
        email -> Nullable<Text>,
        created_at -> Timestamp,
        updated_at -> Timestamp,
        reported -> Bool,
        report_attempts -> Int4,
        next_report_attempt -> Nullable<Timestamp>,
        wallet_tx_id -> Nullable<Int8>,
        wallet_tx_slate_id -> Nullable<Text>,
        message -> Text,
        slate_messages -> Nullable<Array<Text>>,
        knockturn_fee -> Nullable<Int8>,
        transfer_fee -> Nullable<Int8>,
        real_transfer_fee -> Nullable<Int8>,
        transaction_type -> Transaction_type,
        height -> Nullable<Int8>,
        commit -> Nullable<Text>,
        redirect_url -> Nullable<Text>,
    }
}

table! {
    use diesel::sql_types::*;
    use crate::models::Transaction_status;
    use crate::models::Transaction_type;

    txs (slate_id) {
        slate_id -> Text,
        created_at -> Timestamp,
        confirmed -> Bool,
        confirmed_at -> Nullable<Timestamp>,
        fee -> Nullable<Int8>,
        messages -> Array<Text>,
        num_inputs -> Int8,
        num_outputs -> Int8,
        tx_type -> Text,
        order_id -> Uuid,
        updated_at -> Timestamp,
    }
}

joinable!(transactions -> merchants (merchant_id));
joinable!(txs -> transactions (order_id));

allow_tables_to_appear_in_same_query!(
    current_height,
    merchants,
    rates,
    transactions,
    txs,
);
