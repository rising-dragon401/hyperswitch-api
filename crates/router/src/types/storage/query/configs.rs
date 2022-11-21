use diesel::associations::HasTable;
use router_env::tracing::{self, instrument};

use super::generics::{self, ExecuteQuery};
use crate::{
    connection::PgPooledConn,
    core::errors::{self, CustomResult},
    types::storage::{Config, ConfigNew, ConfigUpdate, ConfigUpdateInternal},
};

impl ConfigNew {
    #[instrument(skip(conn))]
    pub async fn insert(self, conn: &PgPooledConn) -> CustomResult<Config, errors::StorageError> {
        generics::generic_insert::<_, _, Config, _>(conn, self, ExecuteQuery::new()).await
    }
}

impl Config {
    #[instrument(skip(conn))]
    pub async fn find_by_key(
        conn: &PgPooledConn,
        key: &str,
    ) -> CustomResult<Self, errors::StorageError> {
        generics::generic_find_by_id::<<Self as HasTable>::Table, _, _>(conn, key.to_owned()).await
    }

    #[instrument(skip(conn))]
    pub async fn update_by_key(
        conn: &PgPooledConn,
        key: &str,
        config_update: ConfigUpdate,
    ) -> CustomResult<Self, errors::StorageError> {
        match generics::generic_update_by_id::<<Self as HasTable>::Table, _, _, Self, _>(
            conn,
            key.to_owned(),
            ConfigUpdateInternal::from(config_update),
            ExecuteQuery::new(),
        )
        .await
        {
            Err(error) => match error.current_context() {
                errors::StorageError::DatabaseError(errors::DatabaseError::NoFieldsToUpdate) => {
                    generics::generic_find_by_id::<<Self as HasTable>::Table, _, _>(
                        conn,
                        key.to_owned(),
                    )
                    .await
                }
                _ => Err(error),
            },
            result => result,
        }
    }
}
