use data_models::payments::payment_intent::{
    PaymentIntent, PaymentIntentInterface, PaymentIntentNew,
};
#[cfg(feature = "olap")]
use data_models::payments::{
    payment_attempt::PaymentAttempt, payment_intent::PaymentIntentFetchConstraints,
};

use super::MockDb;
#[cfg(feature = "olap")]
use crate::types::api;
use crate::{
    core::errors::{self, CustomResult},
    types::storage::{self as types, enums},
};

#[async_trait::async_trait]
impl PaymentIntentInterface for MockDb {
    #[cfg(feature = "olap")]
    async fn filter_payment_intent_by_constraints(
        &self,
        _merchant_id: &str,
        _filters: &PaymentIntentFetchConstraints,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<Vec<PaymentIntent>, errors::DataStorageError> {
        // [#172]: Implement function for `MockDb`
        Err(errors::DataStorageError::MockDbError)?
    }
    #[cfg(feature = "olap")]
    async fn filter_payment_intents_by_time_range_constraints(
        &self,
        _merchant_id: &str,
        _time_range: &api::TimeRange,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<Vec<PaymentIntent>, errors::DataStorageError> {
        // [#172]: Implement function for `MockDb`
        Err(errors::DataStorageError::MockDbError)?
    }
    #[cfg(feature = "olap")]
    async fn get_filtered_payment_intents_attempt(
        &self,
        _merchant_id: &str,
        _constraints: &PaymentIntentFetchConstraints,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> error_stack::Result<Vec<(PaymentIntent, PaymentAttempt)>, errors::DataStorageError> {
        // [#172]: Implement function for `MockDb`
        Err(errors::DataStorageError::MockDbError)?
    }

    #[allow(clippy::panic)]
    async fn insert_payment_intent(
        &self,
        new: PaymentIntentNew,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::DataStorageError> {
        let mut payment_intents = self.payment_intents.lock().await;
        let time = common_utils::date_time::now();
        let payment_intent = PaymentIntent {
            #[allow(clippy::as_conversions)]
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
            business_country: new.business_country,
            business_label: new.business_label,
            active_attempt_id: new.active_attempt_id.to_owned(),
            order_details: new.order_details,
            allowed_payment_method_types: new.allowed_payment_method_types,
            connector_metadata: new.connector_metadata,
            feature_metadata: new.feature_metadata,
            attempt_count: new.attempt_count,
            profile_id: new.profile_id,
        };
        payment_intents.push(payment_intent.clone());
        Ok(payment_intent)
    }

    // safety: only used for testing
    #[allow(clippy::unwrap_used)]
    async fn update_payment_intent(
        &self,
        this: PaymentIntent,
        update: types::PaymentIntentUpdate,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::DataStorageError> {
        let mut payment_intents = self.payment_intents.lock().await;
        let payment_intent = payment_intents
            .iter_mut()
            .find(|item| item.id == this.id)
            .unwrap();
        *payment_intent = update.apply_changeset(this);
        Ok(payment_intent.clone())
    }

    // safety: only used for testing
    #[allow(clippy::unwrap_used)]
    async fn find_payment_intent_by_payment_id_merchant_id(
        &self,
        payment_id: &str,
        merchant_id: &str,
        _storage_scheme: enums::MerchantStorageScheme,
    ) -> CustomResult<PaymentIntent, errors::DataStorageError> {
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
