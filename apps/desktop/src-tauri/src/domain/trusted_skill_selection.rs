use crate::domain::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedSkillSelection {
    tenant_id: String,
    agent_id: String,
    skill_id: String,
    skill_version_id: String,
}

impl TrustedSkillSelection {
    pub(crate) fn new(
        tenant_id: String,
        agent_id: String,
        skill_id: String,
        skill_version_id: String,
    ) -> Result<Self, AppError> {
        for value in [&tenant_id, &agent_id, &skill_id, &skill_version_id] {
            if value.trim().is_empty() || value.chars().count() > 128 {
                return Err(AppError::ValidationFailed);
            }
        }
        Ok(Self {
            tenant_id,
            agent_id,
            skill_id,
            skill_version_id,
        })
    }

    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn skill_id(&self) -> &str {
        &self.skill_id
    }

    pub fn skill_version_id(&self) -> &str {
        &self.skill_version_id
    }
}
