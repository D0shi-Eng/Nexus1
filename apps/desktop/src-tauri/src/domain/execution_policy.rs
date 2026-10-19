use crate::domain::{
    error::AppError,
    models::{AgentRunStatus, AgentStatus},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionIntentReference {
    GovernedExecutionRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub run_id: String,
    pub agent_id: String,
    pub authenticated_tenant_id: String,
    pub authenticated_user_id: String,
    pub intent_reference: ExecutionIntentReference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPolicyInput {
    pub request: ExecutionRequest,
    pub platform_policy: PolicyGate,
    pub tenant_policy: PolicyGate,
    pub run_tenant_id: String,
    pub agent_tenant_id: String,
    pub run_status: AgentRunStatus,
    pub agent_status: AgentStatus,
    pub authorization_granted: bool,
    pub approval_requirement: ApprovalRequirement,
    pub execution_environment: ExecutionEnvironment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyGate {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRequirement {
    NotRequired,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionEnvironment {
    ExecutorUnavailable,
    VerifiedExecutorAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDecisionOutcome {
    Deny,
    RequireApproval,
    ReadyForExecutor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDecisionReason {
    PlatformPolicyDenied,
    TenantPolicyDenied,
    AgentDisabled,
    AuthorizationDenied,
    InvalidRunState,
    TenantScopeViolation,
    ApprovalRequired,
    ExecutionNotAvailable,
    AllGatesSatisfied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionDecision {
    pub outcome: ExecutionDecisionOutcome,
    pub reason: ExecutionDecisionReason,
}

impl ExecutionDecision {
    pub fn deny(reason: ExecutionDecisionReason) -> Self {
        Self {
            outcome: ExecutionDecisionOutcome::Deny,
            reason,
        }
    }

    pub fn require_approval() -> Self {
        Self {
            outcome: ExecutionDecisionOutcome::RequireApproval,
            reason: ExecutionDecisionReason::ApprovalRequired,
        }
    }

    pub fn ready_for_executor() -> Self {
        Self {
            outcome: ExecutionDecisionOutcome::ReadyForExecutor,
            reason: ExecutionDecisionReason::AllGatesSatisfied,
        }
    }
}

pub fn execution_intent_reference_from_summary(
    value: &str,
) -> Result<ExecutionIntentReference, AppError> {
    match value {
        "GOVERNED_EXECUTION_REQUEST" => Ok(ExecutionIntentReference::GovernedExecutionRequest),
        _ => Err(AppError::DataAccess),
    }
}
