pub mod api;
pub mod authentication;
pub mod encryption;
#[cfg(feature = "kms")]
pub mod kms;
pub mod logger;

use std::sync::{atomic, Arc};

use redis_interface::{errors::RedisError, PubSubInterface};

pub use self::{api::*, encryption::*};
use crate::{
    async_spawn,
    connection::{diesel_make_pg_pool, PgPool},
    consts,
    core::errors,
};

#[derive(Clone)]
pub struct Store {
    pub master_pool: PgPool,
    #[cfg(feature = "olap")]
    pub replica_pool: PgPool,
    pub redis_conn: Arc<redis_interface::RedisConnectionPool>,
    #[cfg(feature = "kv_store")]
    pub(crate) config: StoreConfig,
}

#[cfg(feature = "kv_store")]
#[derive(Clone)]
pub(crate) struct StoreConfig {
    pub(crate) drainer_stream_name: String,
    pub(crate) drainer_num_partitions: u8,
}

impl Store {
    pub async fn new(config: &crate::configs::settings::Settings, test_transaction: bool) -> Self {
        let redis_conn = Arc::new(crate::connection::redis_connection(config).await);
        let redis_clone = redis_conn.clone();

        let subscriber_conn = redis_conn.clone();

        redis_conn.subscribe(consts::PUB_SUB_CHANNEL).await.ok();

        async_spawn!({
            if let Err(e) = subscriber_conn.on_message().await {
                logger::error!(pubsub_err=?e);
            }
        });

        async_spawn!({
            redis_clone.on_error().await;
        });

        Self {
            master_pool: diesel_make_pg_pool(&config.master_database, test_transaction).await,
            #[cfg(feature = "olap")]
            replica_pool: diesel_make_pg_pool(&config.replica_database, test_transaction).await,
            redis_conn,
            #[cfg(feature = "kv_store")]
            config: StoreConfig {
                drainer_stream_name: config.drainer.stream_name.clone(),
                drainer_num_partitions: config.drainer.num_partitions,
            },
        }
    }

    #[cfg(feature = "kv_store")]
    pub fn get_drainer_stream_name(&self, shard_key: &str) -> String {
        // Example: {shard_5}_drainer_stream
        format!("{{{}}}_{}", shard_key, self.config.drainer_stream_name,)
    }

    pub fn redis_conn(
        &self,
    ) -> errors::CustomResult<Arc<redis_interface::RedisConnectionPool>, RedisError> {
        if self
            .redis_conn
            .is_redis_available
            .load(atomic::Ordering::SeqCst)
        {
            Ok(self.redis_conn.clone())
        } else {
            Err(RedisError::RedisConnectionError.into())
        }
    }

    #[cfg(feature = "kv_store")]
    pub(crate) async fn push_to_drainer_stream<T>(
        &self,
        redis_entry: storage_models::kv::TypedSql,
        partition_key: crate::utils::storage_partitioning::PartitionKey<'_>,
    ) -> crate::core::errors::CustomResult<(), crate::core::errors::StorageError>
    where
        T: crate::utils::storage_partitioning::KvStorePartition,
    {
        use error_stack::ResultExt;

        let shard_key = T::shard_key(partition_key, self.config.drainer_num_partitions);
        let stream_name = self.get_drainer_stream_name(&shard_key);
        self.redis_conn
            .stream_append_entry(
                &stream_name,
                &redis_interface::RedisEntryId::AutoGeneratedID,
                redis_entry
                    .to_field_value_pairs()
                    .change_context(crate::core::errors::StorageError::KVError)?,
            )
            .await
            .change_context(crate::core::errors::StorageError::KVError)
    }
}
