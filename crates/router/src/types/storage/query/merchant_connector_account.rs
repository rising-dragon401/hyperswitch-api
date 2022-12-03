use diesel::{associations::HasTable, BoolExpressionMethods, ExpressionMethods};
use router_env::tracing::{self, instrument};

use super::generics::{self, ExecuteQuery};
use crate::{
    connection::PgPooledConn,
    core::errors::{self, CustomResult},
    schema::merchant_connector_account::dsl,
    types::storage::{
        MerchantConnectorAccount, MerchantConnectorAccountNew, MerchantConnectorAccountUpdate,
        MerchantConnectorAccountUpdateInternal,
    },
};

impl MerchantConnectorAccountNew {
    #[instrument(skip(conn))]
    pub async fn insert_diesel(
        self,
        conn: &PgPooledConn,
    ) -> CustomResult<MerchantConnectorAccount, errors::StorageError> {
        generics::generic_insert::<_, _, MerchantConnectorAccount, _>(
            conn,
            self,
            ExecuteQuery::new(),
        )
        .await
    }
}

impl MerchantConnectorAccount {
    #[instrument(skip(conn))]
    pub async fn update(
        self,
        conn: &PgPooledConn,
        merchant_connector_account: MerchantConnectorAccountUpdate,
    ) -> CustomResult<Self, errors::StorageError> {
        match generics::generic_update_by_id::<<Self as HasTable>::Table, _, _, Self, _>(
            conn,
            self.id,
            MerchantConnectorAccountUpdateInternal::from(merchant_connector_account),
            ExecuteQuery::new(),
        )
        .await
        {
            Err(error) => match error.current_context() {
                errors::StorageError::DatabaseError(errors::DatabaseError::NoFieldsToUpdate) => {
                    Ok(self)
                }
                _ => Err(error),
            },
            result => result,
        }
    }

    pub async fn delete_by_merchant_id_merchant_connector_id(
        conn: &PgPooledConn,
        merchant_id: &str,
        merchant_connector_id: &i32,
    ) -> CustomResult<bool, errors::StorageError> {
        generics::generic_delete::<<Self as HasTable>::Table, _, _>(
            conn,
            dsl::merchant_id
                .eq(merchant_id.to_owned())
                .and(dsl::merchant_connector_id.eq(merchant_connector_id.to_owned())),
            ExecuteQuery::<Self>::new(),
        )
        .await
    }

    #[instrument(skip(conn))]
    pub async fn find_by_merchant_id_connector(
        conn: &PgPooledConn,
        merchant_id: &str,
        connector: &str,
    ) -> CustomResult<Self, errors::StorageError> {
        generics::generic_find_one::<<Self as HasTable>::Table, _, _>(
            conn,
            dsl::merchant_id
                .eq(merchant_id.to_owned())
                .and(dsl::connector_name.eq(connector.to_owned())),
        )
        .await
    }

    #[instrument(skip(conn))]
    pub async fn find_by_merchant_id_merchant_connector_id(
        conn: &PgPooledConn,
        merchant_id: &str,
        merchant_connector_id: &i32,
    ) -> CustomResult<Self, errors::StorageError> {
        generics::generic_find_one::<<Self as HasTable>::Table, _, _>(
            conn,
            dsl::merchant_id
                .eq(merchant_id.to_owned())
                .and(dsl::merchant_connector_id.eq(merchant_connector_id.to_owned())),
        )
        .await
    }

    #[instrument(skip(conn))]
    pub async fn find_by_merchant_id(
        conn: &PgPooledConn,
        merchant_id: &str,
    ) -> CustomResult<Vec<Self>, errors::StorageError> {
        generics::generic_filter::<<Self as HasTable>::Table, _, _>(
            conn,
            dsl::merchant_id.eq(merchant_id.to_owned()),
            None,
        )
        .await
    }
}
