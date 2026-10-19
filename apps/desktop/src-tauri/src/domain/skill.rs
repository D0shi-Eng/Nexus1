use serde::Serialize;

use crate::domain::error::AppError;

pub const MAX_SKILL_INSTRUCTION_BYTES: usize = 2048;
pub const MAX_SKILL_CONTRACT_BYTES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SkillStatus {
    Active,
    Disabled,
}

impl SkillStatus {
    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "ACTIVE" => Ok(Self::Active),
            "DISABLED" => Ok(Self::Disabled),
            _ => Err(AppError::DataAccess),
        }
    }

    pub fn as_code(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Disabled => "DISABLED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: String,
    pub status: SkillStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionRecord {
    pub id: String,
    pub tenant_id: String,
    pub skill_id: String,
    pub skill_status: SkillStatus,
    pub version_number: i64,
    pub instruction: String,
    pub input_contract: String,
    pub output_contract: String,
    pub constraints: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillInstruction {
    pub version_id: String,
    pub version_number: i64,
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedSkillInstructionSet {
    entries: Vec<SkillInstruction>,
}

impl TrustedSkillInstructionSet {
    pub fn from_versions(mut versions: Vec<SkillVersionRecord>) -> Result<Self, AppError> {
        if versions.len() > 1 {
            return Err(AppError::ValidationFailed);
        }
        versions.sort_by(|left, right| left.id.cmp(&right.id));
        let entries = versions
            .into_iter()
            .map(|version| {
                if version.skill_status == SkillStatus::Disabled {
                    return Err(AppError::InvalidStateTransition);
                }
                validate_bounded_text(&version.instruction, MAX_SKILL_INSTRUCTION_BYTES)?;
                Ok(SkillInstruction {
                    version_id: version.id,
                    version_number: version.version_number,
                    instruction: version.instruction,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[SkillInstruction] {
        &self.entries
    }
}

pub fn validate_skill_identity(name: &str, description: &str) -> Result<(), AppError> {
    if name.trim().is_empty()
        || name.chars().count() > 160
        || description.trim().is_empty()
        || description.chars().count() > 2_000
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

pub fn validate_skill_version(
    version_number: i64,
    instruction: &str,
    input_contract: &str,
    output_contract: &str,
    constraints: &str,
) -> Result<(), AppError> {
    if version_number < 1 {
        return Err(AppError::ValidationFailed);
    }
    validate_bounded_text(instruction, MAX_SKILL_INSTRUCTION_BYTES)?;
    validate_bounded_text(input_contract, MAX_SKILL_CONTRACT_BYTES)?;
    validate_bounded_text(output_contract, MAX_SKILL_CONTRACT_BYTES)?;
    validate_bounded_text(constraints, MAX_SKILL_CONTRACT_BYTES)
}

fn validate_bounded_text(value: &str, max_bytes: usize) -> Result<(), AppError> {
    if value.trim().is_empty() || value.as_bytes().len() > max_bytes {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_skill_identity, validate_skill_version, SkillStatus};

    #[test]
    fn validates_identity_and_small_typed_revision() {
        validate_skill_identity("Risk analysis", "A bounded reasoning capability.")
            .expect("هوية Skill يجب أن تكون صالحة");
        validate_skill_version(
            1,
            "identify risks",
            "required: evidence",
            "risk list",
            "no side effects",
        )
        .expect("نسخة Skill يجب أن تكون صالحة");
        assert_eq!(SkillStatus::Active.as_code(), "ACTIVE");
    }

    #[test]
    fn rejects_empty_or_oversized_values() {
        assert!(validate_skill_identity(" ", "description").is_err());
        assert!(
            validate_skill_version(0, "instruction", "input", "output", "constraints").is_err()
        );
        assert!(
            validate_skill_version(1, &"x".repeat(2049), "input", "output", "constraints").is_err()
        );
    }

    #[test]
    fn disabled_skill_is_rejected_from_trusted_instruction_set() {
        let version = super::SkillVersionRecord {
            id: "version-disabled".to_owned(),
            tenant_id: "tenant-a".to_owned(),
            skill_id: "skill-a".to_owned(),
            skill_status: SkillStatus::Disabled,
            version_number: 1,
            instruction: "do not compile".to_owned(),
            input_contract: "input".to_owned(),
            output_contract: "output".to_owned(),
            constraints: "bounded".to_owned(),
            created_at: "2026-09-22T00:00:00Z".to_owned(),
        };
        assert!(super::TrustedSkillInstructionSet::from_versions(vec![version]).is_err());
    }
}
