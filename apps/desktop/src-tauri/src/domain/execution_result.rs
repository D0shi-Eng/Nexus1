use crate::domain::{
    error::AppError,
    provider::{ModelId, ProviderFailure, ProviderGeneration, ProviderId},
};

pub const MAX_EXECUTION_OUTPUT_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionResultOutcome {
    Completed,
    Failed,
}

impl ExecutionResultOutcome {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionFailureCode {
    ProviderUnavailable,
    ProviderTimeout,
    ModelUnavailable,
    ProviderResponseInvalid,
    ExecutionInfrastructureError,
    ToolAuthorizationDenied,
    ToolInputInvalid,
    ToolCallLimitExceeded,
    ToolExecutionFailed,
    ToolResultTooLarge,
    ExecutionNotConfigured,
}

impl ExecutionFailureCode {
    pub fn from_provider_failure(value: ProviderFailure) -> Self {
        match value {
            ProviderFailure::Unavailable => Self::ProviderUnavailable,
            ProviderFailure::Timeout => Self::ProviderTimeout,
            ProviderFailure::ModelUnavailable => Self::ModelUnavailable,
            ProviderFailure::InputTooLarge
            | ProviderFailure::OutputTooLarge
            | ProviderFailure::InvalidResponse
            | ProviderFailure::ToolCallUnsupported => Self::ProviderResponseInvalid,
            // رفض سياسة الوجهة وغياب السر كلاهما فشل تهيئة تنفيذ لا خطأ استجابة:
            // لا يُخفى كخطأ مزود عابر كي لا يوحي بأن إعادة المحاولة تنفع.
            ProviderFailure::EndpointRejected(_) | ProviderFailure::CredentialMissing => {
                Self::ExecutionNotConfigured
            }
        }
    }

    pub fn as_code(&self) -> &'static str {
        match self {
            Self::ProviderUnavailable => "PROVIDER_UNAVAILABLE",
            Self::ProviderTimeout => "PROVIDER_TIMEOUT",
            Self::ModelUnavailable => "MODEL_UNAVAILABLE",
            Self::ProviderResponseInvalid => "PROVIDER_RESPONSE_INVALID",
            Self::ExecutionInfrastructureError => "EXECUTION_INFRASTRUCTURE_ERROR",
            Self::ToolAuthorizationDenied => "TOOL_AUTHORIZATION_DENIED",
            Self::ToolInputInvalid => "TOOL_INPUT_INVALID",
            Self::ToolCallLimitExceeded => "TOOL_CALL_LIMIT_EXCEEDED",
            Self::ToolExecutionFailed => "TOOL_EXECUTION_FAILED",
            Self::ToolResultTooLarge => "TOOL_RESULT_TOO_LARGE",
            Self::ExecutionNotConfigured => "EXECUTION_NOT_CONFIGURED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "PROVIDER_UNAVAILABLE" => Ok(Self::ProviderUnavailable),
            "PROVIDER_TIMEOUT" => Ok(Self::ProviderTimeout),
            "MODEL_UNAVAILABLE" => Ok(Self::ModelUnavailable),
            "PROVIDER_RESPONSE_INVALID" => Ok(Self::ProviderResponseInvalid),
            "EXECUTION_INFRASTRUCTURE_ERROR" => Ok(Self::ExecutionInfrastructureError),
            "TOOL_AUTHORIZATION_DENIED" => Ok(Self::ToolAuthorizationDenied),
            "TOOL_INPUT_INVALID" => Ok(Self::ToolInputInvalid),
            "TOOL_CALL_LIMIT_EXCEEDED" => Ok(Self::ToolCallLimitExceeded),
            "TOOL_EXECUTION_FAILED" => Ok(Self::ToolExecutionFailed),
            "TOOL_RESULT_TOO_LARGE" => Ok(Self::ToolResultTooLarge),
            "EXECUTION_NOT_CONFIGURED" => Ok(Self::ExecutionNotConfigured),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    outcome: ExecutionResultOutcome,
    provider_id: ProviderId,
    model_id: Option<ModelId>,
    output: Option<String>,
    failure_code: Option<ExecutionFailureCode>,
    provider_generated_at: Option<String>,
}

impl ExecutionResult {
    pub fn completed(
        provider_id: ProviderId,
        generation: ProviderGeneration,
    ) -> Result<Self, AppError> {
        if generation.output.trim().is_empty()
            || generation.output.as_bytes().len() > MAX_EXECUTION_OUTPUT_BYTES
        {
            return Err(AppError::ProviderResponseInvalid);
        }

        Ok(Self {
            outcome: ExecutionResultOutcome::Completed,
            provider_id,
            model_id: Some(generation.model_id),
            output: Some(generation.output),
            failure_code: None,
            provider_generated_at: Some(generation.generated_at),
        })
    }

    pub fn failed(provider_id: ProviderId, failure_code: ExecutionFailureCode) -> Self {
        Self {
            outcome: ExecutionResultOutcome::Failed,
            provider_id,
            model_id: None,
            output: None,
            failure_code: Some(failure_code),
            provider_generated_at: None,
        }
    }

    pub fn outcome(&self) -> ExecutionResultOutcome {
        self.outcome
    }

    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    pub fn model_id(&self) -> Option<&ModelId> {
        self.model_id.as_ref()
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }

    pub fn failure_code(&self) -> Option<ExecutionFailureCode> {
        self.failure_code
    }

    pub fn provider_generated_at(&self) -> Option<&str> {
        self.provider_generated_at.as_deref()
    }

    pub(crate) fn from_persisted(
        outcome: ExecutionResultOutcome,
        provider_id: ProviderId,
        model_id: Option<ModelId>,
        output: Option<String>,
        failure_code: Option<ExecutionFailureCode>,
        provider_generated_at: Option<String>,
    ) -> Result<Self, AppError> {
        match outcome {
            ExecutionResultOutcome::Completed => {
                let Some(model_id) = model_id else {
                    return Err(AppError::DataAccess);
                };
                let Some(output) = output else {
                    return Err(AppError::DataAccess);
                };
                let Some(provider_generated_at) = provider_generated_at else {
                    return Err(AppError::DataAccess);
                };
                if failure_code.is_some()
                    || output.trim().is_empty()
                    || output.as_bytes().len() > MAX_EXECUTION_OUTPUT_BYTES
                {
                    return Err(AppError::DataAccess);
                }

                Ok(Self {
                    outcome,
                    provider_id,
                    model_id: Some(model_id),
                    output: Some(output),
                    failure_code: None,
                    provider_generated_at: Some(provider_generated_at),
                })
            }
            ExecutionResultOutcome::Failed => {
                if model_id.is_some()
                    || output.is_some()
                    || failure_code.is_none()
                    || provider_generated_at.is_some()
                {
                    return Err(AppError::DataAccess);
                }

                Ok(Self {
                    outcome,
                    provider_id,
                    model_id: None,
                    output: None,
                    failure_code,
                    provider_generated_at: None,
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResultRecord {
    pub id: String,
    pub tenant_id: String,
    pub agent_run_id: String,
    pub agent_run_revision: i64,
    pub result: ExecutionResult,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionFailureCode, ExecutionResult, ExecutionResultOutcome, MAX_EXECUTION_OUTPUT_BYTES,
    };
    use crate::domain::provider::{ModelId, ProviderGeneration, ProviderId};

    #[test]
    fn completed_result_retains_only_bounded_provider_provenance() {
        let result = ExecutionResult::completed(
            ProviderId::ollama_local(),
            ProviderGeneration {
                model_id: ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً"),
                output: "trusted result".to_owned(),
                generated_at: "2026-08-24T00:00:00Z".to_owned(),
            },
        )
        .expect("نتيجة المزود يجب أن تكون صالحة");

        assert_eq!(result.outcome(), ExecutionResultOutcome::Completed);
        assert_eq!(result.output(), Some("trusted result"));
        assert_eq!(result.failure_code(), None);
    }

    #[test]
    fn rejects_oversized_completed_output() {
        let result = ExecutionResult::completed(
            ProviderId::ollama_local(),
            ProviderGeneration {
                model_id: ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً"),
                output: "a".repeat(MAX_EXECUTION_OUTPUT_BYTES + 1),
                generated_at: "2026-08-24T00:00:00Z".to_owned(),
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn maps_provider_failures_to_closed_failure_codes() {
        assert_eq!(
            ExecutionFailureCode::from_provider_failure(
                crate::domain::provider::ProviderFailure::Timeout,
            ),
            ExecutionFailureCode::ProviderTimeout
        );
    }
}
