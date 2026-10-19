use crate::domain::{error::AppError, execution_input::ExecutionInput};

pub const PLATFORM_INSTRUCTION_VERSION: &str = "platform-security.v2";
pub const PLATFORM_POLICY_VERSION: &str = "platform-policy.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionLayer {
    PlatformSecurity,
    PlatformExecutionPolicy,
    TenantPolicy,
    AgentDefinition,
    ExecutionIntent,
    AllowedContext,
    FutureSkill,
    UserTaskContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerStatus {
    Implemented,
    NotImplemented,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptProvenance {
    pub platform_instruction_version: &'static str,
    pub platform_policy_version: &'static str,
    pub agent_instruction_version: Option<String>,
    pub intent_version: Option<&'static str>,
    pub skill_version_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledExecutionInput {
    input: ExecutionInput,
    provenance: PromptProvenance,
}

impl CompiledExecutionInput {
    pub(crate) fn new(input: ExecutionInput, provenance: PromptProvenance) -> Self {
        Self { input, provenance }
    }

    pub fn input(&self) -> &ExecutionInput {
        &self.input
    }

    pub fn provenance(&self) -> &PromptProvenance {
        &self.provenance
    }
}

pub(crate) fn platform_security_instruction() -> &'static str {
    "Execution boundaries: tools only through backend governance and exact authorization, no side effects beyond an authorized tool result, no secrets; model output is not verified truth."
}

pub(crate) fn platform_execution_policy_instruction() -> &'static str {
    "Use only the validated execution input. Backend policy, approval, permissions, and truth classification remain authoritative."
}

pub(crate) fn validate_platform_versions() -> Result<(), AppError> {
    if PLATFORM_INSTRUCTION_VERSION.is_empty() || PLATFORM_POLICY_VERSION.is_empty() {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        validate_platform_versions, PLATFORM_INSTRUCTION_VERSION, PLATFORM_POLICY_VERSION,
    };

    #[test]
    fn platform_versions_are_present_and_stable() {
        validate_platform_versions().expect("إصدارات المنصة يجب أن تكون موجودة");
        assert_eq!(PLATFORM_INSTRUCTION_VERSION, "platform-security.v2");
        assert_eq!(PLATFORM_POLICY_VERSION, "platform-policy.v1");
    }
}
