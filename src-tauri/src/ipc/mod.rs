use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IPCEvent {
    SessionStarted { session: crate::claude::Session },
    SessionUpdated { session: crate::claude::Session },
    SessionEnded { session_id: String },
    PermissionRequested { request: crate::claude::PermissionRequest },
    PermissionApproved { request_id: String },
    PermissionDenied { request_id: String },
    PlanReview { session_id: String, plan: String },
    TerminalJumped { tab_id: String },
}
