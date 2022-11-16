use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::storage::process_tracker::ProcessTracker;

#[derive(Debug, Clone)]
pub struct ProcessData {
    db_name: String,
    cache_name: String,
    process_tracker: ProcessTracker,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RetryMapping {
    pub start_after: i32,
    pub frequency: Vec<i32>,
    pub count: Vec<i32>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorPTMapping {
    pub default_mapping: RetryMapping,
    pub custom_merchant_mapping: HashMap<String, RetryMapping>,
    pub max_retries_count: i32,
}
