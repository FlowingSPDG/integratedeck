use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteActionParams {
    pub connection_id: String,
    pub action_id: String,
    pub options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFeedbacksParams {
    pub connection_id: String,
    pub feedback_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackValueUpdate {
    pub id: String,
    pub value: Value,
}
