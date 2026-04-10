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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session() -> crate::claude::Session {
        crate::claude::Session {
            id: "cli-12345".to_string(),
            agent: "claude-code".to_string(),
            title: "test-project".to_string(),
            cwd: "/Users/test/project".to_string(),
            status: "running".to_string(),
            terminal: "iTerm2".to_string(),
            tab_id: "tab-cli-12345".to_string(),
            started_at: 1000,
            last_activity: 1100,
        }
    }

    #[test]
    fn test_session_started_serialization() {
        let event = IPCEvent::SessionStarted { session: test_session() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"session_started\""));
        assert!(json.contains("\"agent\":\"claude-code\""));
    }

    #[test]
    fn test_session_updated_serialization() {
        let event = IPCEvent::SessionUpdated {
            session: crate::claude::Session {
                status: "waiting".to_string(),
                ..test_session()
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"session_updated\""));
        assert!(json.contains("\"status\":\"waiting\""));
    }

    #[test]
    fn test_session_ended_serialization() {
        let event = IPCEvent::SessionEnded {
            session_id: "cli-999".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"session_ended\""));
        assert!(json.contains("\"session_id\":\"cli-999\""));
    }

    #[test]
    fn test_permission_requested_serialization() {
        let event = IPCEvent::PermissionRequested {
            request: crate::claude::PermissionRequest {
                id: "req-1".to_string(),
                session_id: "cli-100".to_string(),
                request_type: "tool_use".to_string(),
                tool_name: Some("Write".to_string()),
                message: "Allow write?".to_string(),
                options: None,
                timestamp: 2000,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"permission_requested\""));
        assert!(json.contains("\"requestType\":\"tool_use\""));
    }

    #[test]
    fn test_permission_approved_serialization() {
        let event = IPCEvent::PermissionApproved {
            request_id: "req-1".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"permission_approved\""));
    }

    #[test]
    fn test_permission_denied_serialization() {
        let event = IPCEvent::PermissionDenied {
            request_id: "req-1".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"permission_denied\""));
    }

    #[test]
    fn test_plan_review_serialization() {
        let event = IPCEvent::PlanReview {
            session_id: "cli-50".to_string(),
            plan: "Refactor module X".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"plan_review\""));
        assert!(json.contains("\"plan\":\"Refactor module X\""));
    }

    #[test]
    fn test_terminal_jumped_serialization() {
        let event = IPCEvent::TerminalJumped {
            tab_id: "tab-cli-12345".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"terminal_jumped\""));
        assert!(json.contains("\"tab_id\":\"tab-cli-12345\""));
    }

    #[test]
    fn test_deserialization_roundtrip() {
        let event = IPCEvent::SessionStarted { session: test_session() };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: IPCEvent = serde_json::from_str(&json).unwrap();

        if let IPCEvent::SessionStarted { session } = parsed {
            assert_eq!(session.id, "cli-12345");
            assert_eq!(session.agent, "claude-code");
        } else {
            panic!("Expected SessionStarted variant");
        }
    }

    #[test]
    fn test_all_variants_have_unique_type_tags() {
        let events = [
            serde_json::to_string(&IPCEvent::SessionStarted { session: test_session() }).unwrap(),
            serde_json::to_string(&IPCEvent::SessionUpdated { session: test_session() }).unwrap(),
            serde_json::to_string(&IPCEvent::SessionEnded { session_id: "x".into() }).unwrap(),
            serde_json::to_string(&IPCEvent::PermissionRequested {
                request: crate::claude::PermissionRequest {
                    id: "r".into(), session_id: "s".into(), request_type: "t".into(),
                    tool_name: None, message: "m".into(), options: None, timestamp: 0,
                },
            }).unwrap(),
            serde_json::to_string(&IPCEvent::PermissionApproved { request_id: "x".into() }).unwrap(),
            serde_json::to_string(&IPCEvent::PermissionDenied { request_id: "x".into() }).unwrap(),
            serde_json::to_string(&IPCEvent::PlanReview { session_id: "x".into(), plan: "p".into() }).unwrap(),
            serde_json::to_string(&IPCEvent::TerminalJumped { tab_id: "x".into() }).unwrap(),
        ];

        let types: Vec<String> = events.iter().map(|e| {
            let v: serde_json::Value = serde_json::from_str(e).unwrap();
            v.get("type").unwrap().as_str().unwrap().to_string()
        }).collect();

        let tags = ["session_started", "session_updated", "session_ended",
                     "permission_requested", "permission_approved", "permission_denied",
                     "plan_review", "terminal_jumped"];

        for (i, tag) in tags.iter().enumerate() {
            assert_eq!(types[i], *tag, "Variant {} has wrong tag", i);
        }
    }
}
