use crate::domain::tool::{
    ToolEffectProfile, ToolExternality, ToolMutation, ToolPrivilege, ToolReversibility,
};

/// سياسة تفويض الأدوات — دالة حتمية نقية على سياق مطبوع؛ لا IO ولا حالة.
/// البوابات أولاً (كلها إلزامية) ثم تصنيف الأثر — لا درجة خطورة رقمية مُبهمة.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAuthorizationContext {
    pub agent_active: bool,
    pub tool_active: bool,
    pub assignment_active: bool,
    pub trusted_adapter_available: bool,
    pub underlying_permission_granted: bool,
    pub effect: ToolEffectProfile,
    /// موافقة APPROVED قائمة على الهوية الدقيقة (نسخة+هاش مدخل+تشغيل+مراجعة).
    pub approved_for_exact_identity: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuthorizationDecision {
    Allow(ToolAuthorizationReason),
    RequireApproval(ToolAuthorizationReason),
    Deny(ToolAuthorizationReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAuthorizationReason {
    AgentInactive,
    ToolInactive,
    AssignmentInactive,
    NoTrustedAdapter,
    UnderlyingPermissionMissing,
    EffectPrivileged,
    EffectExternalIrreversible,
    EffectRequiresApproval,
    ApprovedIdentityBound,
    SafeReadOnlyInternalTool,
}

impl ToolAuthorizationReason {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::AgentInactive => "AGENT_INACTIVE",
            Self::ToolInactive => "TOOL_INACTIVE",
            Self::AssignmentInactive => "ASSIGNMENT_INACTIVE",
            Self::NoTrustedAdapter => "NO_TRUSTED_ADAPTER",
            Self::UnderlyingPermissionMissing => "UNDERLYING_PERMISSION_MISSING",
            Self::EffectPrivileged => "EFFECT_PRIVILEGED",
            Self::EffectExternalIrreversible => "EFFECT_EXTERNAL_IRREVERSIBLE",
            Self::EffectRequiresApproval => "EFFECT_REQUIRES_APPROVAL",
            Self::ApprovedIdentityBound => "APPROVED_IDENTITY_BOUND",
            Self::SafeReadOnlyInternalTool => "SAFE_READ_ONLY_INTERNAL_TOOL",
        }
    }
}

impl ToolAuthorizationDecision {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Allow(_) => "ALLOW",
            Self::RequireApproval(_) => "REQUIRE_APPROVAL",
            Self::Deny(_) => "DENY",
        }
    }

    pub fn reason_code(self) -> &'static str {
        match self {
            Self::Allow(reason) | Self::RequireApproval(reason) | Self::Deny(reason) => {
                reason.as_code()
            }
        }
    }
}

pub fn evaluate_tool_authorization(
    context: &ToolAuthorizationContext,
) -> ToolAuthorizationDecision {
    use ToolAuthorizationDecision as Decision;
    use ToolAuthorizationReason as Reason;

    // البوابات — فشل أي بوابة يعني الرفض الفوري (فشل مغلق).
    if !context.agent_active {
        return Decision::Deny(Reason::AgentInactive);
    }
    if !context.tool_active {
        return Decision::Deny(Reason::ToolInactive);
    }
    if !context.assignment_active {
        return Decision::Deny(Reason::AssignmentInactive);
    }
    if !context.trusted_adapter_available {
        return Decision::Deny(Reason::NoTrustedAdapter);
    }
    if !context.underlying_permission_granted {
        return Decision::Deny(Reason::UnderlyingPermissionMissing);
    }

    // تصنيف الأثر — محاور صريحة لا مجموع رقمي.
    let effect = &context.effect;
    if effect.privilege == ToolPrivilege::Privileged {
        return Decision::Deny(Reason::EffectPrivileged);
    }
    if effect.externality == ToolExternality::External
        && effect.reversibility == ToolReversibility::Irreversible
    {
        return Decision::Deny(Reason::EffectExternalIrreversible);
    }

    let is_safe_internal_read = effect.mutation == ToolMutation::None
        && effect.externality == ToolExternality::Internal
        && effect.privilege == ToolPrivilege::Standard
        && effect.reversibility == ToolReversibility::NotApplicable;

    if is_safe_internal_read {
        return Decision::Allow(Reason::SafeReadOnlyInternalTool);
    }
    if context.approved_for_exact_identity {
        return Decision::Allow(Reason::ApprovedIdentityBound);
    }
    Decision::RequireApproval(Reason::EffectRequiresApproval)
}

#[cfg(test)]
mod tests {
    use super::{evaluate_tool_authorization, ToolAuthorizationContext, ToolAuthorizationDecision};
    use crate::domain::tool::{
        ToolEffectProfile, ToolExternality, ToolMutation, ToolPrivilege, ToolReversibility,
    };

    fn context(effect: ToolEffectProfile) -> ToolAuthorizationContext {
        ToolAuthorizationContext {
            agent_active: true,
            tool_active: true,
            assignment_active: true,
            trusted_adapter_available: true,
            underlying_permission_granted: true,
            effect,
            approved_for_exact_identity: false,
        }
    }

    fn read_only_effect() -> ToolEffectProfile {
        ToolEffectProfile {
            mutation: ToolMutation::None,
            externality: ToolExternality::Internal,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::NotApplicable,
        }
    }

    #[test]
    fn safe_internal_read_only_tool_is_allowed() {
        let decision = evaluate_tool_authorization(&context(read_only_effect()));
        assert_eq!(decision.as_code(), "ALLOW");
        assert_eq!(decision.reason_code(), "SAFE_READ_ONLY_INTERNAL_TOOL");
    }

    #[test]
    fn every_failed_gate_denies_before_effect_evaluation() {
        let mut denied = context(read_only_effect());
        denied.agent_active = false;
        assert_eq!(
            evaluate_tool_authorization(&denied).reason_code(),
            "AGENT_INACTIVE"
        );
        let mut denied = context(read_only_effect());
        denied.tool_active = false;
        assert_eq!(
            evaluate_tool_authorization(&denied).reason_code(),
            "TOOL_INACTIVE"
        );
        let mut denied = context(read_only_effect());
        denied.assignment_active = false;
        assert_eq!(
            evaluate_tool_authorization(&denied).reason_code(),
            "ASSIGNMENT_INACTIVE"
        );
        let mut denied = context(read_only_effect());
        denied.trusted_adapter_available = false;
        assert_eq!(
            evaluate_tool_authorization(&denied).reason_code(),
            "NO_TRUSTED_ADAPTER"
        );
        let mut denied = context(read_only_effect());
        denied.underlying_permission_granted = false;
        assert_eq!(
            evaluate_tool_authorization(&denied).reason_code(),
            "UNDERLYING_PERMISSION_MISSING"
        );
    }

    #[test]
    fn higher_effect_profiles_never_auto_execute() {
        let state_change = ToolEffectProfile {
            mutation: ToolMutation::StateChange,
            externality: ToolExternality::Internal,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::Reversible,
        };
        assert_eq!(
            evaluate_tool_authorization(&context(state_change)).as_code(),
            "REQUIRE_APPROVAL"
        );

        let external_reversible = ToolEffectProfile {
            mutation: ToolMutation::StateChange,
            externality: ToolExternality::External,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::Reversible,
        };
        assert_eq!(
            evaluate_tool_authorization(&context(external_reversible)).as_code(),
            "REQUIRE_APPROVAL"
        );
    }

    #[test]
    fn privileged_or_external_irreversible_is_denied() {
        let privileged = ToolEffectProfile {
            mutation: ToolMutation::StateChange,
            externality: ToolExternality::Internal,
            privilege: ToolPrivilege::Privileged,
            reversibility: ToolReversibility::Reversible,
        };
        assert_eq!(
            evaluate_tool_authorization(&context(privileged)).as_code(),
            "DENY"
        );

        let external_irreversible = ToolEffectProfile {
            mutation: ToolMutation::StateChange,
            externality: ToolExternality::External,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::Irreversible,
        };
        assert_eq!(
            evaluate_tool_authorization(&context(external_irreversible)).as_code(),
            "DENY"
        );
    }

    #[test]
    fn exact_approved_identity_promotes_require_approval_to_allow() {
        let state_change = ToolEffectProfile {
            mutation: ToolMutation::StateChange,
            externality: ToolExternality::Internal,
            privilege: ToolPrivilege::Standard,
            reversibility: ToolReversibility::Conditional,
        };
        let mut approved = context(state_change);
        approved.approved_for_exact_identity = true;
        let decision = evaluate_tool_authorization(&approved);
        assert_eq!(decision.as_code(), "ALLOW");
        assert_eq!(decision.reason_code(), "APPROVED_IDENTITY_BOUND");
    }

    #[test]
    fn approval_never_rescues_a_failed_gate_or_denied_effect() {
        let mut no_adapter = context(read_only_effect());
        no_adapter.trusted_adapter_available = false;
        no_adapter.approved_for_exact_identity = true;
        assert_eq!(evaluate_tool_authorization(&no_adapter).as_code(), "DENY");

        let privileged = ToolEffectProfile {
            mutation: ToolMutation::StateChange,
            externality: ToolExternality::Internal,
            privilege: ToolPrivilege::Privileged,
            reversibility: ToolReversibility::Reversible,
        };
        let mut approved = context(privileged);
        approved.approved_for_exact_identity = true;
        assert_eq!(evaluate_tool_authorization(&approved).as_code(), "DENY");
    }

    #[test]
    fn decision_is_deterministic_and_typed() {
        let first = evaluate_tool_authorization(&context(read_only_effect()));
        let second = evaluate_tool_authorization(&context(read_only_effect()));
        assert_eq!(first, second);
        assert!(matches!(first, ToolAuthorizationDecision::Allow(_)));
    }
}
