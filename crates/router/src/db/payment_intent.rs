use super::MockDb;
use crate::{
    core::errors::{self, CustomResult},
    types::{
        api,
        storage::{enums, PaymentIntent, PaymentIntentNew, PaymentIntentUpdate},
    },
};

#[async_trait::async_trait]
pub trait PaymentIntentInterface {
    async fn update_payment_intent(
        &self,
        this: PaymentIntent,
        payment_intent: PaymentIntentUpdate,
        storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::StorageError>;

    async fn insert_payment_intent(
        &self,
        new: PaymentIntentNew,
        storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::StorageError>;

    async fn find_payment_intent_by_payment_id_merchant_id(
        &self,
        payment_id: &str,
        merchant_id: &str,
        storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::StorageError>;

    async fn filter_payment_intent_by_constraints(
        &self,
        merchant_id: &str,
        pc: &api::PaymentListConstraints,
        storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<Vec<PaymentIntent>, errors::StorageError>;
}

#[cfg(feature = "kv_store")]
mod storage {
    use common_utils::date_time;
    use error_stack::{IntoReport, ResultExt};
    use fred::prelude::{RedisErrorKind, *};
    use redis_interface::RedisEntryId;

    use super::PaymentIntentInterface;
    use crate::{
        connection::pg_connection,
        core::errors::{self, CustomResult},
        services::Store,
        types::{
            api,
            storage::{enums, payment_intent::*},
        },
        utils::storage_partitioning::KvStorePartition,
    };

    #[async_trait::async_trait]
    impl PaymentIntentInterface for Store {
        async fn insert_payment_intent(
            &self,
            new: PaymentIntentNew,
            storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<PaymentIntent, errors::StorageError> {
            match storage_scheme {
                enums::MerchantStorageScheme::PostgresOnly => {
                    let conn = pg_connection(&self.master_pool).await;
                    new.insert_diesel(&conn).await
                }

                enums::MerchantStorageScheme::RedisKv => {
                    let key = format!("{}_{}", new.payment_id, new.merchant_id);
                    let created_intent = PaymentIntent {
                        id: 0i32,
                        payment_id: new.payment_id.clone(),
                        merchant_id: new.merchant_id.clone(),
                        status: new.status,
                        amount: new.amount,
                        currency: new.currency,
                        amount_captured: new.amount_captured,
                        customer_id: new.customer_id.clone(),
                        description: new.description.clone(),
                        return_url: new.return_url.clone(),
                        metadata: new.metadata.clone(),
                        connector_id: new.connector_id.clone(),
                        shipping_address_id: new.shipping_address_id.clone(),
                        billing_address_id: new.billing_address_id.clone(),
                        statement_descriptor_name: new.statement_descriptor_name.clone(),
                        statement_descriptor_suffix: new.statement_descriptor_suffix.clone(),
                        created_at: new.created_at.unwrap_or_else(date_time::now),
                        modified_at: new.created_at.unwrap_or_else(date_time::now),
                        last_synced: new.last_synced,
                        setup_future_usage: new.setup_future_usage,
                        off_session: new.off_session,
                        client_secret: new.client_secret.clone(),
                    };
                    // TODO: Add a proper error for serialization failure
                    let redis_value = serde_json::to_string(&created_intent)
                        .into_report()
                        .change_context(errors::StorageError::KVError)?;
                    match self
                        .redis_conn
                        .pool
                        .hsetnx::<u8, &str, &str, &str>(&key, "pi", &redis_value)
                        .await
                    {
                        Ok(0) => Err(errors::StorageError::DuplicateValue(format!(
                            "Payment Intent already exists for payment_id: {key}"
                        )))
                        .into_report(),
                        Ok(1) => {
                            let conn = pg_connection(&self.master_pool).await;
                            let query = new
                                .insert_diesel_query(&conn)
                                .await
                                .change_context(errors::StorageError::KVError)?;
                            let stream_name = self.drainer_stream(&PaymentIntent::shard_key(
                                crate::utils::storage_partitioning::PartitionKey::MerchantIdPaymentId {
                                    merchant_id: &created_intent.merchant_id,
                                    payment_id: &created_intent.payment_id,
                                },
                                self.config.drainer_num_partitions,
                            ));
                            self.redis_conn
                                .stream_append_entry(
                                    &stream_name,
                                    &RedisEntryId::AutoGeneratedID,
                                    query.to_field_value_pairs(),
                                )
                                .await
                                .change_context(errors::StorageError::KVError)?;
                            Ok(created_intent)
                        }
                        Ok(i) => Err(errors::StorageError::KVError)
                            .into_report()
                            .attach_printable_lazy(|| {
                                format!("Invalid response for HSETNX: {}", i)
                            }),
                        Err(er) => Err(er)
                            .into_report()
                            .change_context(errors::StorageError::KVError),
                    }
                }
            }
        }

        async fn update_payment_intent(
            &self,
            this: PaymentIntent,
            payment_intent: PaymentIntentUpdate,
            storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<PaymentIntent, errors::StorageError> {
            match storage_scheme {
                enums::MerchantStorageScheme::PostgresOnly => {
                    let conn = pg_connection(&self.master_pool).await;
                    this.update(&conn, payment_intent).await
                }

                enums::MerchantStorageScheme::RedisKv => {
                    let key = format!("{}_{}", this.payment_id, this.merchant_id);

                    let updated_intent = payment_intent.clone().apply_changeset(this.clone());
                    // Check for database presence as well Maybe use a read replica here ?
                    // TODO: Add a proper error for serialization failure
                    let redis_value = serde_json::to_string(&updated_intent)
                        .into_report()
                        .change_context(errors::StorageError::KVError)?;
                    let updated_intent = self
                        .redis_conn
                        .pool
                        .hset::<u8, &str, (&str, String)>(&key, ("pi", redis_value))
                        .await
                        .map(|_| updated_intent)
                        .into_report()
                        .change_context(errors::StorageError::KVError)?;

                    let conn = pg_connection(&self.master_pool).await;
                    let query = this
                        .update_query(&conn, payment_intent)
                        .await
                        .change_context(errors::StorageError::KVError)?;
                    let stream_name = self.drainer_stream(&PaymentIntent::shard_key(
                        crate::utils::storage_partitioning::PartitionKey::MerchantIdPaymentId {
                            merchant_id: &updated_intent.merchant_id,
                            payment_id: &updated_intent.payment_id,
                        },
                        self.config.drainer_num_partitions,
                    ));
                    self.redis_conn
                        .stream_append_entry(
                            &stream_name,
                            &RedisEntryId::AutoGeneratedID,
                            query.to_field_value_pairs(),
                        )
                        .await
                        .change_context(errors::StorageError::KVError)?;
                    Ok(updated_intent)
                }
            }
        }

        async fn find_payment_intent_by_payment_id_merchant_id(
            &self,
            payment_id: &str,
            merchant_id: &str,
            storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<PaymentIntent, errors::StorageError> {
            match storage_scheme {
                enums::MerchantStorageScheme::PostgresOnly => {
                    let conn = pg_connection(&self.master_pool).await;
                    PaymentIntent::find_by_payment_id_merchant_id(&conn, payment_id, merchant_id)
                        .await
                }

                enums::MerchantStorageScheme::RedisKv => {
                    let key = format!("{}_{}", payment_id, merchant_id);
                    self.redis_conn
                        .pool
                        .hget::<String, &str, &str>(&key, "pi")
                        .await
                        .map_err(|err| match err.kind() {
                            RedisErrorKind::NotFound => errors::StorageError::ValueNotFound(
                                format!("Payment Intent does not exist for {}", key),
                            ),
                            _ => errors::StorageError::KVError,
                        })
                        .into_report()
                        .and_then(|redis_resp| {
                            serde_json::from_str::<PaymentIntent>(&redis_resp)
                                .into_report()
                                .change_context(errors::StorageError::KVError)
                        })
                    // Check for database presence as well Maybe use a read replica here ?
                }
            }
        }

        async fn filter_payment_intent_by_constraints(
            &self,
            merchant_id: &str,
            pc: &api::PaymentListConstraints,
            storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<Vec<PaymentIntent>, errors::StorageError> {
            match storage_scheme {
                enums::MerchantStorageScheme::PostgresOnly => {
                    let conn = pg_connection(&self.master_pool).await;
                    PaymentIntent::filter_by_constraints(&conn, merchant_id, pc).await
                }

                enums::MerchantStorageScheme::RedisKv => {
                    //TODO: Implement this
                    Err(errors::StorageError::KVError.into())
                }
            }
        }
    }
}

#[cfg(not(feature = "kv_store"))]
mod storage {
    use super::PaymentIntentInterface;
    use crate::{
        connection::pg_connection,
        core::errors::{self, CustomResult},
        services::Store,
        types::{
            api,
            storage::{enums, payment_intent::*},
        },
    };

    #[async_trait::async_trait]
    impl PaymentIntentInterface for Store {
        async fn insert_payment_intent(
            &self,
            new: PaymentIntentNew,
            _storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<PaymentIntent, errors::StorageError> {
            let conn = pg_connection(&self.master_pool).await;
            new.insert_diesel(&conn).await
        }

        async fn update_payment_intent(
            &self,
            this: PaymentIntent,
            payment_intent: PaymentIntentUpdate,
            _storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<PaymentIntent, errors::StorageError> {
            let conn = pg_connection(&self.master_pool).await;
            this.update(&conn, payment_intent).await
        }

        async fn find_payment_intent_by_payment_id_merchant_id(
            &self,
            payment_id: &str,
            merchant_id: &str,
            _storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<PaymentIntent, errors::StorageError> {
            let conn = pg_connection(&self.master_pool).await;
            PaymentIntent::find_by_payment_id_merchant_id(&conn, payment_id, merchant_id).await
        }

        async fn filter_payment_intent_by_constraints(
            &self,
            merchant_id: &str,
            pc: &api::PaymentListConstraints,
            _storage_scheme: enums::MerchantStorageScheme,
        ) -> CustomResult<Vec<PaymentIntent>, errors::StorageError> {
            let conn = pg_connection(&self.master_pool).await;
            PaymentIntent::filter_by_constraints(&conn, merchant_id, pc).await
        }
    }
}

#[async_trait::async_trait]
impl PaymentIntentInterface for MockDb {
    async fn filter_payment_intent_by_constraints(
        &self,
        _merchant_id: &str,
        _pc: &api::PaymentListConstraints,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<Vec<PaymentIntent>, errors::StorageError> {
        todo!()
    }

    #[allow(clippy::panic)]
    async fn insert_payment_intent(
        &self,
        new: PaymentIntentNew,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::StorageError> {
        let mut payment_intents = self.payment_intents.lock().await;
        let time = common_utils::date_time::now();
        let payment_intent = PaymentIntent {
            id: payment_intents.len() as i32,
            payment_id: new.payment_id,
            merchant_id: new.merchant_id,
            status: new.status,
            amount: new.amount,
            currency: new.currency,
            amount_captured: new.amount_captured,
            customer_id: new.customer_id,
            description: new.description,
            return_url: new.return_url,
            metadata: new.metadata,
            connector_id: new.connector_id,
            shipping_address_id: new.shipping_address_id,
            billing_address_id: new.billing_address_id,
            statement_descriptor_name: new.statement_descriptor_name,
            statement_descriptor_suffix: new.statement_descriptor_suffix,
            created_at: new.created_at.unwrap_or(time),
            modified_at: new.modified_at.unwrap_or(time),
            last_synced: new.last_synced,
            setup_future_usage: new.setup_future_usage,
            off_session: new.off_session,
            client_secret: new.client_secret,
        };
        payment_intents.push(payment_intent.clone());
        Ok(payment_intent)
    }

    async fn update_payment_intent(
        &self,
        this: PaymentIntent,
        update: PaymentIntentUpdate,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::StorageError> {
        let mut payment_intents = self.payment_intents.lock().await;
        let payment_intent = payment_intents
            .iter_mut()
            .find(|item| item.id == this.id)
            .unwrap();
        *payment_intent = update.apply_changeset(this);
        Ok(payment_intent.clone())
    }

    async fn find_payment_intent_by_payment_id_merchant_id(
        &self,
        payment_id: &str,
        merchant_id: &str,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::StorageError> {
        let payment_intents = self.payment_intents.lock().await;

        Ok(payment_intents
            .iter()
            .find(|payment_intent| {
                payment_intent.payment_id == payment_id && payment_intent.merchant_id == merchant_id
            })
            .cloned()
            .unwrap())
    }
}
