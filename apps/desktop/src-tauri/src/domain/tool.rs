use serde::{Deserialize, Serialize};

use crate::domain::error::AppError;

pub const MAX_TOOL_NAME_CHARS: usize = 160;
pub const MAX_TOOL_DESCRIPTION_CHARS: usize = 2_000;
pub const MAX_CONTRACT_FIELDS: usize = 64;
pub const MAX_CONTRACT_FIELD_NAME_CHARS: usize = 64;
pub const MAX_CONTRACT_FIELD_DESCRIPTION_CHARS: usize = 256;
pub const MAX_CONTRACT_BYTES: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolStatus {
    Active,
    Disabled,
}

impl ToolStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolMutation {
    None,
    StateChange,
}

impl ToolMutation {
    fn as_code(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::StateChange => "STATE_CHANGE",
        }
    }

    fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "NONE" => Ok(Self::None),
            "STATE_CHANGE" => Ok(Self::StateChange),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolExternality {
    Internal,
    External,
}

impl ToolExternality {
    fn as_code(self) -> &'static str {
        match self {
            Self::Internal => "INTERNAL",
            Self::External => "EXTERNAL",
        }
    }

    fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "INTERNAL" => Ok(Self::Internal),
            "EXTERNAL" => Ok(Self::External),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolPrivilege {
    Standard,
    Privileged,
}

impl ToolPrivilege {
    fn as_code(self) -> &'static str {
        match self {
            Self::Standard => "STANDARD",
            Self::Privileged => "PRIVILEGED",
        }
    }

    fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "STANDARD" => Ok(Self::Standard),
            "PRIVILEGED" => Ok(Self::Privileged),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolReversibility {
    NotApplicable,
    Reversible,
    Conditional,
    Irreversible,
}

impl ToolReversibility {
    fn as_code(self) -> &'static str {
        match self {
            Self::NotApplicable => "NOT_APPLICABLE",
            Self::Reversible => "REVERSIBLE",
            Self::Conditional => "CONDITIONAL",
            Self::Irreversible => "IRREVERSIBLE",
        }
    }

    fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "NOT_APPLICABLE" => Ok(Self::NotApplicable),
            "REVERSIBLE" => Ok(Self::Reversible),
            "CONDITIONAL" => Ok(Self::Conditional),
            "IRREVERSIBLE" => Ok(Self::Irreversible),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolEffectProfile {
    pub mutation: ToolMutation,
    pub externality: ToolExternality,
    pub privilege: ToolPrivilege,
    pub reversibility: ToolReversibility,
}

impl ToolEffectProfile {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.mutation == ToolMutation::None
            && self.reversibility != ToolReversibility::NotApplicable
        {
            return Err(AppError::ValidationFailed);
        }
        if self.mutation == ToolMutation::StateChange
            && self.reversibility == ToolReversibility::NotApplicable
        {
            return Err(AppError::ValidationFailed);
        }
        Ok(())
    }

    pub fn codes(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        (
            self.mutation.as_code(),
            self.externality.as_code(),
            self.privilege.as_code(),
            self.reversibility.as_code(),
        )
    }

    pub(crate) fn from_codes(
        mutation: &str,
        externality: &str,
        privilege: &str,
        reversibility: &str,
    ) -> Result<Self, AppError> {
        let profile = Self {
            mutation: ToolMutation::from_code(mutation)?,
            externality: ToolExternality::from_code(externality)?,
            privilege: ToolPrivilege::from_code(privilege)?,
            reversibility: ToolReversibility::from_code(reversibility)?,
        };
        profile.validate()?;
        Ok(profile)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolContractField {
    pub name: String,
    pub data_type: String,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolContract {
    pub fields: Vec<ToolContractField>,
    pub allows_additional_fields: bool,
}

impl ToolContract {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.fields.len() > MAX_CONTRACT_FIELDS {
            return Err(AppError::ValidationFailed);
        }
        for field in &self.fields {
            if field.name.trim().is_empty()
                || field.name.chars().count() > MAX_CONTRACT_FIELD_NAME_CHARS
                || field.data_type.trim().is_empty()
                || field.data_type.chars().count() > MAX_CONTRACT_FIELD_NAME_CHARS
                || field.description.chars().count() > MAX_CONTRACT_FIELD_DESCRIPTION_CHARS
                || contains_authority_field_name(&field.name)
            {
                return Err(AppError::ValidationFailed);
            }
        }
        let encoded = serde_json::to_vec(self).map_err(|_| AppError::ValidationFailed)?;
        if encoded.len() > MAX_CONTRACT_BYTES {
            return Err(AppError::ValidationFailed);
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<String, AppError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| AppError::ValidationFailed)
    }

    pub fn from_json(value: &str) -> Result<Self, AppError> {
        let contract: Self = serde_json::from_str(value).map_err(|_| AppError::DataAccess)?;
        contract.validate()?;
        Ok(contract)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: String,
    pub status: ToolStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionRecord {
    pub id: String,
    pub tenant_id: String,
    pub tool_id: String,
    pub tool_status: ToolStatus,
    pub version_number: i64,
    pub input_contract: ToolContract,
    pub output_contract: ToolContract,
    pub effect: ToolEffectProfile,
    pub created_at: String,
}

pub fn validate_tool_identity(name: &str, description: &str) -> Result<(), AppError> {
    if name.trim().is_empty()
        || name.chars().count() > MAX_TOOL_NAME_CHARS
        || description.trim().is_empty()
        || description.chars().count() > MAX_TOOL_DESCRIPTION_CHARS
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

pub fn validate_tool_version(
    version_number: i64,
    input_contract: &ToolContract,
    output_contract: &ToolContract,
    effect: &ToolEffectProfile,
) -> Result<(), AppError> {
    if version_number < 1 {
        return Err(AppError::ValidationFailed);
    }
    input_contract.validate()?;
    output_contract.validate()?;
    effect.validate()
}

fn contains_authority_field_name(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "tenant_id"
            | "permission"
            | "approval"
            | "execution_policy"
            | "provider_authority"
            | "secret"
            | "credential"
            | "token"
            | "api_key"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> ToolContract {
        ToolContract {
            fields: vec![ToolContractField {
                name: "document_id".to_owned(),
                data_type: "string".to_owned(),
                required: true,
                description: "Bounded document identifier".to_owned(),
            }],
            allows_additional_fields: false,
        }
    }

    fn read_effect() -> ToolEffectProfile {
        ToolEffectProfile {
            mutation: ToolMutation::None,
            externality: ToolExternality::Internal,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::NotApplicable,
        }
    }

    #[test]
    fn validates_identity_and_typed_read_effect() {
        validate_tool_identity("Evidence reader", "Reads a declared evidence resource.")
            .expect("هوية Tool يجب أن تكون صالحة");
        validate_tool_version(1, &contract(), &contract(), &read_effect())
            .expect("نسخة Tool يجب أن تكون صالحة");
    }

    #[test]
    fn rejects_authority_fields_and_invalid_effect_combinations() {
        let mut invalid = contract();
        invalid.fields[0].name = "permission".to_owned();
        assert!(invalid.validate().is_err());
        let invalid_effect = ToolEffectProfile {
            mutation: ToolMutation::None,
            externality: ToolExternality::External,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::Reversible,
        };
        assert!(invalid_effect.validate().is_err());
    }
}
