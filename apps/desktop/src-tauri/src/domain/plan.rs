use serde::Serialize;

use crate::domain::error::AppError;

// الحدود الصلبة للمخطط والتنفيذ متعدد الوكلاء (§53/§69/C-10/C-11)
pub const MAX_PLAN_STEPS: usize = 4;
pub const MAX_DELEGATIONS_PER_PLAN: usize = 4;
pub const MAX_PROVIDER_CALLS_PER_PLAN: usize = 12;
pub const MAX_PLAN_GOAL_CHARS: usize = 500;
pub const MAX_STEP_OBJECTIVE_CHARS: usize = 500;
pub const MAX_EVIDENCE_REQUIREMENT_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanStatus {
    PendingApproval,
    Active,
    Running,
    Completed,
    Failed,
    Rejected,
    Cancelled,
}

impl PlanStatus {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Active => "ACTIVE",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Rejected => "REJECTED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "PENDING_APPROVAL" => Ok(Self::PendingApproval),
            "ACTIVE" => Ok(Self::Active),
            "RUNNING" => Ok(Self::Running),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "REJECTED" => Ok(Self::Rejected),
            "CANCELLED" => Ok(Self::Cancelled),
            _ => Err(AppError::DataAccess),
        }
    }

    // تُستخدم في اختبارات الحالات النهائية الحتمية — المنتج يعتمد CAS الصريح
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Rejected | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanStepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl PlanStepStatus {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "RUNNING" => Ok(Self::Running),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "CANCELLED" => Ok(Self::Cancelled),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DelegationStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl DelegationStatus {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "RUNNING" => Ok(Self::Running),
            "COMPLETED" => Ok(Self::Completed),
            "FAILED" => Ok(Self::Failed),
            "CANCELLED" => Ok(Self::Cancelled),
            _ => Err(AppError::DataAccess),
        }
    }

    // تُستخدم في اختبارات الحالات النهائية الحتمية — المنتج يعتمد CAS الصريح
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

// قدرة الوكيل: طلب قدرة من تعداد مغلق — المخطط لا يسمي أدوات ولا هويات (C-10)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentCapability {
    Research,
    Compliance,
    Risk,
    Evidence,
    Cybersecurity,
    Synthesis,
}

impl AgentCapability {
    // العرض النصي جزء من عقد القدرة؛ التحقق المنتجي يعتمد from_code (رفض المجهول)
    #[allow(dead_code)]
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Research => "RESEARCH",
            Self::Compliance => "COMPLIANCE",
            Self::Risk => "RISK",
            Self::Evidence => "EVIDENCE",
            Self::Cybersecurity => "CYBERSECURITY",
            Self::Synthesis => "SYNTHESIS",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "RESEARCH" => Ok(Self::Research),
            "COMPLIANCE" => Ok(Self::Compliance),
            "RISK" => Ok(Self::Risk),
            "EVIDENCE" => Ok(Self::Evidence),
            "CYBERSECURITY" => Ok(Self::Cybersecurity),
            "SYNTHESIS" => Ok(Self::Synthesis),
            _ => Err(AppError::DataAccess),
        }
    }
}

// خطوة مقترحة من المخطط قبل التحقق — كل شيء يُتحقق قبل القبول
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposedPlanStep {
    pub objective: String,
    pub required_capability: String,
    pub requires_knowledge: bool,
    pub evidence_requirement: String,
}

pub fn validate_proposed_steps(steps: &[ProposedPlanStep]) -> Result<(), AppError> {
    if steps.is_empty() || steps.len() > MAX_PLAN_STEPS {
        return Err(AppError::PlanTooLarge);
    }
    for step in steps {
        if step.objective.trim().is_empty()
            || step.objective.chars().count() > MAX_STEP_OBJECTIVE_CHARS
        {
            return Err(AppError::PlanInvalid);
        }
        AgentCapability::from_code(&step.required_capability).map_err(|_| AppError::PlanInvalid)?;
        if step.evidence_requirement.trim().is_empty()
            || step.evidence_requirement.chars().count() > MAX_EVIDENCE_REQUIREMENT_CHARS
        {
            return Err(AppError::PlanInvalid);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanRecord {
    pub id: String,
    pub tenant_id: String,
    pub goal: String,
    pub status: PlanStatus,
    pub steps_hash: String,
    pub model_id_at_request: Option<String>,
    pub revision: i64,
    pub requested_by_user_id: String,
    pub synthesis_output: Option<String>,
    pub conflict_flag: bool,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanStepRecord {
    pub id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub step_index: usize,
    pub objective: String,
    pub required_capability: String,
    pub requires_knowledge: bool,
    pub evidence_requirement: String,
    pub status: PlanStepStatus,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentDelegationRecord {
    pub id: String,
    pub tenant_id: String,
    pub plan_id: String,
    pub plan_revision: i64,
    pub step_id: String,
    pub target_agent_id: String,
    pub agent_run_id: Option<String>,
    pub objective: String,
    pub allowed_skill_version_id: Option<String>,
    pub status: DelegationStatus,
    pub revision: i64,
    pub output: Option<String>,
    pub citations_json: Option<String>,
    pub truth_class: Option<String>,
    pub failure_code: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        validate_proposed_steps, AgentCapability, DelegationStatus, PlanStatus, PlanStepStatus,
        ProposedPlanStep, MAX_PLAN_STEPS,
    };

    fn step(capability: &str) -> ProposedPlanStep {
        ProposedPlanStep {
            objective: "Analyze the bounded obligations".to_owned(),
            required_capability: capability.to_owned(),
            requires_knowledge: false,
            evidence_requirement: "Cite retrieved sources".to_owned(),
        }
    }

    #[test]
    fn bounded_valid_steps_pass_validation() {
        let steps: Vec<ProposedPlanStep> = (0..MAX_PLAN_STEPS).map(|_| step("RESEARCH")).collect();
        assert!(validate_proposed_steps(&steps).is_ok());
    }

    #[test]
    fn empty_or_oversized_step_sets_are_rejected() {
        assert!(validate_proposed_steps(&[]).is_err());
        let oversized: Vec<ProposedPlanStep> =
            (0..=MAX_PLAN_STEPS).map(|_| step("RESEARCH")).collect();
        assert!(validate_proposed_steps(&oversized).is_err());
    }

    #[test]
    fn unknown_capability_is_rejected_from_model_output() {
        assert!(validate_proposed_steps(&[step("ROOT_SHELL")]).is_err());
        assert!(validate_proposed_steps(&[step("ALL_TOOLS")]).is_err());
        assert!(AgentCapability::from_code("ADMIN").is_err());
    }

    #[test]
    fn terminal_states_stay_terminal_across_plan_and_delegation() {
        assert!(PlanStatus::Completed.is_terminal());
        assert!(!PlanStatus::Active.is_terminal());
        assert!(DelegationStatus::Failed.is_terminal());
        assert!(!DelegationStatus::Pending.is_terminal());
        assert_eq!(
            PlanStepStatus::from_code("RUNNING").unwrap().as_code(),
            "RUNNING"
        );
    }
}
