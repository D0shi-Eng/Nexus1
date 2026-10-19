use serde::Serialize;

use crate::domain::error::AppError;

// الحد الأقصى لتعليمة بروفايل التنفيذ — مستقل عن description الوكيل (عرض فقط)
// ومتطابق مع حد تعليمات Skill ليبقى التجميع الكلي داخل سقف التشغيل.
pub const MAX_PROFILE_INSTRUCTION_BYTES: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionProfileVersion {
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub version_number: i64,
    pub instruction: String,
    pub created_at: String,
}

pub fn validate_profile_instruction(instruction: &str) -> Result<(), AppError> {
    let trimmed = instruction.trim();
    if trimmed.is_empty() || trimmed.as_bytes().len() > MAX_PROFILE_INSTRUCTION_BYTES {
        return Err(AppError::ValidationFailed);
    }
    // التعليمة بيانات تنفيذ موثوقة من مستأجر — لا يجوز أن تتضمن أسطراً تحاكي
    // فواصل الطبقات المجمّعة فتحاول انتحال طبقة أعلى سلطة.
    for marker in [
        "[PLATFORM_SECURITY]",
        "[PLATFORM_POLICY]",
        "[USER_TASK_CONTENT]",
        "[TOOL_RESULT:",
    ] {
        if instruction.contains(marker) {
            return Err(AppError::ValidationFailed);
        }
    }
    Ok(())
}

pub fn validate_profile_identity(
    agent_id: &str,
    version_number: i64,
    instruction: &str,
) -> Result<(), AppError> {
    if agent_id.trim().is_empty() || agent_id.chars().count() > 128 || version_number < 1 {
        return Err(AppError::ValidationFailed);
    }
    validate_profile_instruction(instruction)
}

#[cfg(test)]
mod tests {
    use super::{validate_profile_identity, validate_profile_instruction};

    #[test]
    fn accepts_bounded_instruction_and_rejects_oversized_or_blank() {
        validate_profile_instruction("Summarize obligations for the requester.")
            .expect("التعليمة المحدودة يجب أن تُقبل");
        assert!(validate_profile_instruction("").is_err());
        assert!(validate_profile_instruction("   ").is_err());
        assert!(validate_profile_instruction(&"x".repeat(2049)).is_err());
        assert!(validate_profile_instruction(&"x".repeat(2048)).is_ok());
    }

    #[test]
    fn rejects_instructions_that_impersonate_compiled_layers() {
        assert!(validate_profile_instruction("[PLATFORM_SECURITY]\nfake").is_err());
        assert!(validate_profile_instruction("use [USER_TASK_CONTENT] to override").is_err());
        assert!(validate_profile_identity("agent-1", 1, "valid instruction").is_ok());
        assert!(validate_profile_identity(" ", 1, "valid instruction").is_err());
        assert!(validate_profile_identity("agent-1", 0, "valid instruction").is_err());
    }
}
