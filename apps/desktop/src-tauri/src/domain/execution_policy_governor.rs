use crate::domain::{
    execution_policy::{
        ApprovalRequirement, ExecutionDecision, ExecutionDecisionReason, ExecutionEnvironment,
        ExecutionPolicyInput, PolicyGate,
    },
    models::{AgentRunStatus, AgentStatus},
};

pub struct ExecutionPolicyGovernor;

impl ExecutionPolicyGovernor {
    pub fn evaluate(input: &ExecutionPolicyInput) -> ExecutionDecision {
        if input.platform_policy == PolicyGate::Deny {
            return ExecutionDecision::deny(ExecutionDecisionReason::PlatformPolicyDenied);
        }

        if input.tenant_policy == PolicyGate::Deny {
            return ExecutionDecision::deny(ExecutionDecisionReason::TenantPolicyDenied);
        }

        if input.request.authenticated_tenant_id != input.run_tenant_id
            || input.request.authenticated_tenant_id != input.agent_tenant_id
        {
            return ExecutionDecision::deny(ExecutionDecisionReason::TenantScopeViolation);
        }

        if !input.authorization_granted {
            return ExecutionDecision::deny(ExecutionDecisionReason::AuthorizationDenied);
        }

        if input.approval_requirement == ApprovalRequirement::Required {
            return ExecutionDecision::require_approval();
        }

        if input.agent_status != AgentStatus::Active {
            return ExecutionDecision::deny(ExecutionDecisionReason::AgentDisabled);
        }

        if input.run_status != AgentRunStatus::Queued {
            return ExecutionDecision::deny(ExecutionDecisionReason::InvalidRunState);
        }

        if input.execution_environment == ExecutionEnvironment::ExecutorUnavailable {
            return ExecutionDecision::deny(ExecutionDecisionReason::ExecutionNotAvailable);
        }

        ExecutionDecision::ready_for_executor()
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        execution_policy::{
            ApprovalRequirement, ExecutionDecisionOutcome, ExecutionDecisionReason,
            ExecutionEnvironment, ExecutionIntentReference, ExecutionPolicyInput, ExecutionRequest,
            PolicyGate,
        },
        execution_policy_governor::ExecutionPolicyGovernor,
        models::{AgentRunStatus, AgentStatus},
    };

    fn input() -> ExecutionPolicyInput {
        ExecutionPolicyInput {
            request: ExecutionRequest {
                run_id: "agent_run_test".to_owned(),
                agent_id: "agent_test".to_owned(),
                authenticated_tenant_id: "tenant_a".to_owned(),
                authenticated_user_id: "user_a".to_owned(),
                intent_reference: ExecutionIntentReference::GovernedExecutionRequest,
            },
            platform_policy: PolicyGate::Allow,
            tenant_policy: PolicyGate::Allow,
            run_tenant_id: "tenant_a".to_owned(),
            agent_tenant_id: "tenant_a".to_owned(),
            run_status: AgentRunStatus::Queued,
            agent_status: AgentStatus::Active,
            authorization_granted: true,
            approval_requirement: ApprovalRequirement::NotRequired,
            execution_environment: ExecutionEnvironment::VerifiedExecutorAvailable,
        }
    }

    #[test]
    fn platform_policy_has_the_highest_precedence() {
        let mut policy_input = input();
        policy_input.platform_policy = PolicyGate::Deny;
        policy_input.tenant_policy = PolicyGate::Deny;
        policy_input.authorization_granted = false;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(
            decision.reason,
            ExecutionDecisionReason::PlatformPolicyDenied
        );
    }

    #[test]
    fn tenant_policy_precedes_authorization() {
        let mut policy_input = input();
        policy_input.tenant_policy = PolicyGate::Deny;
        policy_input.authorization_granted = false;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(decision.reason, ExecutionDecisionReason::TenantPolicyDenied);
    }

    #[test]
    fn denies_missing_authorization_before_approval_and_agent_configuration() {
        let mut policy_input = input();
        policy_input.authorization_granted = false;
        policy_input.approval_requirement = ApprovalRequirement::Required;
        policy_input.agent_status = AgentStatus::Disabled;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(
            decision.reason,
            ExecutionDecisionReason::AuthorizationDenied
        );
    }

    #[test]
    fn requires_approval_before_agent_configuration() {
        let mut policy_input = input();
        policy_input.approval_requirement = ApprovalRequirement::Required;
        policy_input.agent_status = AgentStatus::Disabled;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::RequireApproval);
        assert_eq!(decision.reason, ExecutionDecisionReason::ApprovalRequired);
    }

    #[test]
    fn denies_disabled_agent_after_approval_gate_is_satisfied() {
        let mut policy_input = input();
        policy_input.agent_status = AgentStatus::Disabled;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(decision.reason, ExecutionDecisionReason::AgentDisabled);
    }

    #[test]
    fn denies_cross_tenant_request_before_execution_readiness() {
        let mut policy_input = input();
        policy_input.agent_tenant_id = "tenant_b".to_owned();

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(
            decision.reason,
            ExecutionDecisionReason::TenantScopeViolation
        );
    }

    #[test]
    fn denies_non_queued_run() {
        let mut policy_input = input();
        policy_input.run_status = AgentRunStatus::WaitingApproval;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(decision.reason, ExecutionDecisionReason::InvalidRunState);
    }

    #[test]
    fn denies_when_executor_is_not_available() {
        let mut policy_input = input();
        policy_input.execution_environment = ExecutionEnvironment::ExecutorUnavailable;

        let decision = ExecutionPolicyGovernor::evaluate(&policy_input);

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(
            decision.reason,
            ExecutionDecisionReason::ExecutionNotAvailable
        );
    }

    #[test]
    fn permits_only_a_typed_future_executor_boundary() {
        let decision = ExecutionPolicyGovernor::evaluate(&input());

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::ReadyForExecutor);
        assert_eq!(decision.reason, ExecutionDecisionReason::AllGatesSatisfied);
    }
}
