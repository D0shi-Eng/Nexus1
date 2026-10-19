use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::domain::{
    error::AppError,
    tool::{ToolContract, ToolContractField},
};

pub const MAX_TOOL_INPUT_BYTES: usize = 2_048;
pub const MAX_TOOL_INPUT_STRING_BYTES: usize = 512;
pub const MAX_TOOL_RESULT_OUTPUT_BYTES: usize = 1_024;

// أنواع البيانات المدعومة في مدخلات الاستدعاء — مغلق حتمياً.
const SUPPORTED_DATA_TYPES: [&str; 4] = ["string", "integer", "number", "boolean"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolInvocationStatus {
    WaitingApproval,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ToolInvocationStatus {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::WaitingApproval => "WAITING_APPROVAL",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "WAITING_APPROVAL" => Ok(Self::WaitingApproval),
            "RUNNING" => Ok(Self::Running),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "CANCELLED" => Ok(Self::Cancelled),
            _ => Err(AppError::DataAccess),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

pub fn is_valid_tool_invocation_transition(
    from: ToolInvocationStatus,
    to: ToolInvocationStatus,
) -> bool {
    matches!(
        (from, to),
        (
            ToolInvocationStatus::WaitingApproval,
            ToolInvocationStatus::Running
        ) | (
            ToolInvocationStatus::WaitingApproval,
            ToolInvocationStatus::Cancelled
        ) | (
            ToolInvocationStatus::Running,
            ToolInvocationStatus::Completed
        ) | (ToolInvocationStatus::Running, ToolInvocationStatus::Failed)
            | (
                ToolInvocationStatus::Running,
                ToolInvocationStatus::Cancelled
            )
    )
}

/// مدخل استدعاء مقنن: كائن مسطح محقق ضد عقد الأداة ثم متسلسل بمفاتيح مرتبة —
/// التجزئة (hash) تُحسب على التمثيل المقنن فقط فلا يمكن استبدال المدخل بعد الموافقة.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalToolInput {
    canonical_json: String,
    input_hash: String,
}

impl CanonicalToolInput {
    pub fn validate_and_canonicalize(
        arguments: &serde_json::Value,
        contract: &ToolContract,
    ) -> Result<Self, AppError> {
        // الكائن الفارغ صالح لعقود بلا حقول إلزامية (قيم افتراضية للمحوّل)؛
        // الحقول الإلزامية الناقصة تُرفض في التحقق أدناه حصراً.
        let Some(object) = arguments.as_object() else {
            return Err(AppError::ValidationFailed);
        };

        for field in &contract.fields {
            validate_declared_field(field, object.get(&field.name))?;
        }
        for key in object.keys() {
            if !contract.fields.iter().any(|field| field.name == *key)
                && !contract.allows_additional_fields
            {
                return Err(AppError::ValidationFailed);
            }
        }

        // serde_json بدون ميزة preserve_order يرتب المفاتيح — التمثيل حتمي.
        let canonical_json =
            serde_json::to_string(arguments).map_err(|_| AppError::ValidationFailed)?;
        if canonical_json.as_bytes().len() > MAX_TOOL_INPUT_BYTES {
            return Err(AppError::ValidationFailed);
        }
        let input_hash = format!("{:x}", Sha256::digest(canonical_json.as_bytes()));
        Ok(Self {
            canonical_json,
            input_hash,
        })
    }

    pub fn as_json(&self) -> &str {
        &self.canonical_json
    }

    pub fn hash(&self) -> &str {
        &self.input_hash
    }
}

fn validate_declared_field(
    field: &ToolContractField,
    value: Option<&serde_json::Value>,
) -> Result<(), AppError> {
    let Some(value) = value else {
        if field.required {
            return Err(AppError::ValidationFailed);
        }
        return Ok(());
    };
    if !SUPPORTED_DATA_TYPES.contains(&field.data_type.as_str()) {
        return Err(AppError::DataAccess);
    }
    let matches_type = match field.data_type.as_str() {
        "string" => {
            value.is_string()
                && value
                    .as_str()
                    .is_some_and(|text| text.as_bytes().len() <= MAX_TOOL_INPUT_STRING_BYTES)
        }
        "integer" => value.is_i64(),
        "number" => value.is_number() && value.as_f64().is_some_and(|number| number.is_finite()),
        "boolean" => value.is_boolean(),
        _ => false,
    };
    if !matches_type {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocationRecord {
    pub id: String,
    pub tenant_id: String,
    pub agent_run_id: String,
    pub agent_run_revision: i64,
    pub tool_id: String,
    pub tool_version_id: String,
    pub adapter_key: Option<String>,
    pub input_json: String,
    pub input_hash: String,
    pub policy_decision: String,
    pub policy_reason: String,
    pub status: ToolInvocationStatus,
    pub revision: i64,
    pub failure_code: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocationResultRecord {
    pub id: String,
    pub tenant_id: String,
    pub tool_invocation_id: String,
    pub outcome: String,
    pub output_json: Option<String>,
    pub failure_code: Option<String>,
    pub created_at: String,
}

pub fn validate_bounded_tool_output(output: &str) -> Result<(), AppError> {
    if output.trim().is_empty() || output.as_bytes().len() > MAX_TOOL_RESULT_OUTPUT_BYTES {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        is_valid_tool_invocation_transition, CanonicalToolInput, ToolInvocationStatus,
        MAX_TOOL_RESULT_OUTPUT_BYTES,
    };
    use crate::domain::tool::{ToolContract, ToolContractField};

    fn contract(allows_additional: bool) -> ToolContract {
        ToolContract {
            fields: vec![
                ToolContractField {
                    name: "query".to_owned(),
                    data_type: "string".to_owned(),
                    required: false,
                    description: "filter".to_owned(),
                },
                ToolContractField {
                    name: "limit".to_owned(),
                    data_type: "integer".to_owned(),
                    required: false,
                    description: "bound".to_owned(),
                },
            ],
            allows_additional_fields: allows_additional,
        }
    }

    #[test]
    fn canonicalizes_sorted_input_and_hashes_stably() {
        let first = CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"limit": 5, "query": "privacy"}),
            &contract(false),
        )
        .expect("المدخل الصالح يجب أن يُقنن");
        let second = CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"query": "privacy", "limit": 5}),
            &contract(false),
        )
        .expect("ترتيب المفاتيح لا يغير التقنين");
        assert_eq!(first.as_json(), r#"{"limit":5,"query":"privacy"}"#);
        assert_eq!(first.hash(), second.hash());
    }

    #[test]
    fn rejects_missing_required_unknown_fields_and_wrong_types() {
        let mut required = contract(false);
        required.fields[0].required = true;
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"limit": 1}),
            &required
        )
        .is_err());
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"query":"x","extra":1}),
            &contract(false)
        )
        .is_err());
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"limit":"many"}),
            &contract(false)
        )
        .is_err());
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!("not-an-object"),
            &contract(false)
        )
        .is_err());
        // الكائن الفارغ صالح لعقد اختياري بالكامل — استدعاء بالقيم الافتراضية.
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({}),
            &contract(false)
        )
        .is_ok());
    }

    #[test]
    fn rejects_oversized_strings() {
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"query": "x".repeat(513)}),
            &contract(false)
        )
        .is_err());
        assert!(CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"query": "x".repeat(512)}),
            &contract(false)
        )
        .is_ok());
    }

    #[test]
    fn lifecycle_only_allows_forward_transitions() {
        use ToolInvocationStatus::*;
        assert!(is_valid_tool_invocation_transition(
            WaitingApproval,
            Running
        ));
        assert!(is_valid_tool_invocation_transition(Running, Completed));
        assert!(!is_valid_tool_invocation_transition(Completed, Running));
        assert!(!is_valid_tool_invocation_transition(Failed, Running));
        assert!(!is_valid_tool_invocation_transition(
            Cancelled,
            WaitingApproval
        ));
        assert!(!is_valid_tool_invocation_transition(
            WaitingApproval,
            Completed
        ));
        assert!(Completed.is_terminal());
        assert!(!Running.is_terminal());
    }

    #[test]
    fn bounded_output_limit_is_enforced() {
        assert!(super::validate_bounded_tool_output("{\"items\":[]}").is_ok());
        assert!(super::validate_bounded_tool_output("").is_err());
        assert!(
            super::validate_bounded_tool_output(&"x".repeat(MAX_TOOL_RESULT_OUTPUT_BYTES + 1))
                .is_err()
        );
    }
}
