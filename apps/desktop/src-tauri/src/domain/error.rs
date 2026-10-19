use serde::Serialize;

use crate::domain::provider::ProviderFailure;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("تعذر تهيئة مخزن البيانات المحلي")]
    StorageInitialization,
    #[error("تعذر تنفيذ عملية التخزين")]
    StorageOperation,
    #[error("تعذر قراءة بيانات مساحة العمل")]
    DataAccess,
    #[error("العملية غير مصرح بها في سياق المستخدم الحالي")]
    AuthorizationDenied,
    #[error("بيانات الإدخال لا تستوفي قواعد التحقق")]
    ValidationFailed,
    #[error("المورد المطلوب غير موجود ضمن مساحة العمل الحالية")]
    ResourceNotFound,
    #[error("تعارضت العملية مع تعديل أحدث للمورد")]
    Conflict,
    #[error("انتقال الحالة غير مسموح")]
    InvalidStateTransition,
    #[error("الوكيل معطل ولا يمكنه بدء طلب تشغيل")]
    AgentDisabled,
    #[error("لا يجوز لصاحب الطلب اعتماد طلبه")]
    SelfApprovalForbidden,
    #[error("طلب الموافقة لم يعد يطابق نسخة المورد أو حالته الحالية")]
    StaleApproval,
    #[error("مزود النموذج المحلي غير متاح")]
    ProviderUnavailable,
    #[error("تجاوز مزود النموذج المهلة المسموحة")]
    ProviderTimeout,
    #[error("استجابة مزود النموذج غير صالحة")]
    ProviderResponseInvalid,
    #[error("النموذج المطلوب غير متاح محلياً")]
    ModelUnavailable,
    #[error("تنفيذ التشغيل غير مهيأ أو انعدم ربطه الثابت")]
    ExecutionNotConfigured,
    #[error("مصدر المعرفة لا يستوفي قواعد الإدخال")]
    KnowledgeSourceInvalid,
    #[error("محتوى المعرفة يتجاوز الحد المسموح")]
    KnowledgeSourceTooLarge,
    #[error("محتوى المعرفة مُدخل مسبقاً بنفس الهوية")]
    KnowledgeDuplicate,
    #[error("تعذر استخراج نص المستند")]
    KnowledgeExtractionFailed,
    #[error("تعذر تنفيذ استرجاع المعرفة")]
    KnowledgeRetrievalFailed,
    #[error("رفضت سياسة الذاكرة عملية الكتابة")]
    MemoryWriteDenied,
    #[error("نطاق الذاكرة غير صالح")]
    MemoryScopeInvalid,
    #[error("الخطة غير صالحة بعد التحقق")]
    PlanInvalid,
    #[error("الخطة تتجاوز الحدود الصلبة المسموحة")]
    PlanTooLarge,
    #[error("قدرة الخطة غير متاحة أو غير مصرح بها")]
    PlanCapabilityDenied,
    #[error("رفض التفويض")]
    DelegationDenied,
    #[error("تجاوز التفويض الحد المسموح")]
    DelegationLimitExceeded,
    #[error("اقتباس غير صالح")]
    CitationInvalid,
    #[error("أدلة التحقق غير كافية")]
    VerificationInsufficientEvidence,
    #[error("بيانات الدخول غير صحيحة")]
    AuthInvalidCredentials,
    #[error("الحساب مقفل مؤقتاً بسبب محاولات دخول متكررة")]
    AuthLocked,
    #[error("تهيئة بيئة الإنتاج غير متاحة في الوضع الحالي")]
    BootstrapUnavailable,
    #[error("بيانات تهيئة الإنتاج لا تستوفي القواعد")]
    BootstrapInvalid,
    // ===== GAP-05-OPEN: رمزا رفض مستقلان لبيانات المؤسسة المعلنة من المستخدم =====
    // الفصل مقصود: الموظف يجب أن يعرف أن الخلل في الدولة أو في القطاع بعينه
    // لا أن يرى رسالة عامة لا تدله على الحقل الناقص.
    #[error("رمز الدولة ليس رمزاً معتمداً في كتالوج ISO 3166-1 alpha-2")]
    BootstrapCountryInvalid,
    #[error("اسم القطاع لا يستوفي قواعد الإدخال (طول أو محارف أو عنصر نائب محجوز)")]
    BootstrapIndustryInvalid,
    #[error("لا يجوز تعديل صلاحيات الحساب نفسه")]
    SelfModificationForbidden,
    #[error("ملف النسخة الاحتياطية غير صالح")]
    BackupInvalid,
    #[error("النسخة الاحتياطية تالفة أو معدّلة")]
    BackupCorrupted,
    #[error("نسخة احتياطية بإصدار مخطط غير متوافق")]
    BackupIncompatible,
    #[error("تعذر تنفيذ عملية النسخ الاحتياطي في وحدة التخزين")]
    BackupStorageError,
    // ===== 0023: سجل الموفرين والأسرار وسياسة الوجهة =====
    #[error("رفضت سياسة الوجهة الشبكية العنوان المقدَّم")]
    ProviderEndpointRejected(&'static str),
    #[error("الموفر المطلوب غير معرف في مساحة العمل الحالية")]
    ProviderNotConfigured,
    #[error("الموفر المطلوب معطّل ولا يمكن استعماله")]
    ProviderDisabled,
    #[error("لا توجد مادة سرية محفوظة لهذا الموفر")]
    ProviderCredentialMissing,
    #[error("تعذر تنفيذ عملية مخزن الأسرار الآمن")]
    ProviderCredentialStorageUnavailable,
    #[error("النموذج المختار غير متاح من الموفر المختار")]
    ProviderModelSelectionInvalid,
    // ===== التقارير المحلية: رفض وجهة التصدير وفشل كتابتها =====
    #[error("وجهة ملف التصدير مرفوضة")]
    ReportExportRejected,
    #[error("تعذر كتابة ملف التصدير على وحدة التخزين")]
    ReportExportFailed,
    // ===== 0025: دورة حياة المهمة المحكومة =====
    #[error("لا يمكن طلب اعتماد إنجاز المهمة بلا دليل مسجَّل على التزامها")]
    TaskEvidenceRequired,
    #[error("المهمة مستندة إلى مستخدم غير نشط في هذه المؤسسة")]
    TaskAssignmentInvalid,
    #[error("لا يجوز التحقق البشري من التزام بلا دليل مسجَّل عليه")]
    ObligationVerificationRequiresEvidence,
    // ===== 0026: إدارة المستخدمين الإنتاجية وكلمات المرور =====
    // البريد هوية دخول فريدة داخل المستأجر (UNIQUE (tenant_id, email))؛ الرفض
    // برمز خاص يمنع تحويل خطأ قيد التخزين إلى رسالة عامة لا تدل على الحقل.
    #[error("البريد الإلكتروني مستخدم بالفعل في هذه المؤسسة")]
    UserEmailExists,
    // منح دور يتجاوز ما يملكه المُسنِد رفعُ صلاحيات غير مصرح به — يُرفض خلفياً.
    #[error("الدور المطلوب يمنح صلاحيات تتجاوز صلاحيات المُسنِد")]
    UserRoleNotGrantable,
    #[error("الدور المطلوب غير موجود في كتالوج أدوار المؤسسة")]
    UserRoleUnknown,
    #[error("كلمة المرور الحالية غير صحيحة")]
    CurrentPasswordInvalid,
    #[error("كلمة المرور الجديدة لا تستوفي سياسة كلمة المرور")]
    PasswordPolicyViolation,
    // ===== 0026: فصل من يقدّم الدليل عن من يقرر حالته =====
    #[error("لا يجوز لمقدّم الدليل تغيير حالة التحقق منه")]
    EvidenceSelfVerificationForbidden,
    // ===== 0026: تأليف المصادر التنظيمية والالتزامات =====
    #[error("بيانات المصدر التنظيمي لا تستوفي قواعد الإدخال")]
    RegulatorySourceInvalid,
    #[error("المصدر التنظيمي مسجَّل مسبقاً بنفس الجهة والوثيقة والإصدار")]
    RegulatorySourceDuplicate,
    #[error("المصدر التنظيمي المرجعي غير موجود في مساحة العمل الحالية")]
    ObligationSourceInvalid,
    // ===== جولة الإغلاق النهائية: ضابط آخر مسؤول فعال =====
    // المؤسسة المحلية بلا مسار استرداد خارجي، فبقاء حساب نشط واحد يملك إدارة
    // المستخدمين شرط استمرارية لا رفاهية: تعطيله أو نزع دوره يُقفل المؤسسة نهائياً.
    #[error("العملية ستترك المؤسسة بلا مسؤول نشط قادر على إدارتها")]
    LastAdministratorProtected,
    // ===== جولة الإغلاق النهائية: دورة حياة صلاحية الدليل =====
    // الدليل المنتهي لا يجوز أن يُعتمد: قيمته التعاقدية/التدقيقية سقطت بانتهاء أجله.
    #[error("انتهت صلاحية الدليل فلا يجوز اعتماده")]
    EvidenceExpired,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeError {
    pub code: &'static str,
    pub message: &'static str,
}

impl From<AppError> for SafeError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::StorageInitialization => Self {
                code: "STORAGE_ERROR",
                message: "Local storage is unavailable.",
            },
            AppError::StorageOperation => Self {
                code: "STORAGE_ERROR",
                message: "The local operation could not be completed.",
            },
            AppError::DataAccess => Self {
                code: "STORAGE_ERROR",
                message: "Local workspace data could not be loaded.",
            },
            AppError::AuthorizationDenied => Self {
                code: "AUTHORIZATION_DENIED",
                message: "The requested operation is not authorized.",
            },
            AppError::ValidationFailed => Self {
                code: "VALIDATION_FAILED",
                message: "The provided input is not valid.",
            },
            AppError::ResourceNotFound => Self {
                code: "RESOURCE_NOT_FOUND",
                message: "The requested resource is not available in this workspace.",
            },
            AppError::Conflict => Self {
                code: "CONFLICT",
                message: "The resource has changed. Refresh and try again.",
            },
            AppError::InvalidStateTransition => Self {
                code: "INVALID_STATE_TRANSITION",
                message: "The requested status transition is not allowed.",
            },
            AppError::AgentDisabled => Self {
                code: "AGENT_DISABLED",
                message: "The selected agent is disabled.",
            },
            AppError::SelfApprovalForbidden => Self {
                code: "SELF_APPROVAL_FORBIDDEN",
                message: "The requester cannot decide this approval.",
            },
            AppError::StaleApproval => Self {
                code: "STALE_APPROVAL",
                message: "The approval no longer matches the current resource state.",
            },
            AppError::ProviderUnavailable => Self {
                code: "PROVIDER_UNAVAILABLE",
                message: "The local model provider is unavailable.",
            },
            AppError::ProviderTimeout => Self {
                code: "PROVIDER_TIMEOUT",
                message: "The local model provider did not respond in time.",
            },
            AppError::ProviderResponseInvalid => Self {
                code: "PROVIDER_RESPONSE_INVALID",
                message: "The local model provider returned an invalid response.",
            },
            AppError::ModelUnavailable => Self {
                code: "MODEL_UNAVAILABLE",
                message: "No approved local model is available.",
            },
            AppError::ExecutionNotConfigured => Self {
                code: "EXECUTION_NOT_CONFIGURED",
                message: "Agent execution is not configured for this run.",
            },
            AppError::KnowledgeSourceInvalid => Self {
                code: "KNOWLEDGE_SOURCE_INVALID",
                message: "The knowledge source content is not valid.",
            },
            AppError::KnowledgeSourceTooLarge => Self {
                code: "KNOWLEDGE_SOURCE_TOO_LARGE",
                message: "The knowledge source content exceeds the allowed limit.",
            },
            AppError::KnowledgeDuplicate => Self {
                code: "KNOWLEDGE_DUPLICATE",
                message: "This knowledge content already exists in the workspace.",
            },
            AppError::KnowledgeExtractionFailed => Self {
                code: "KNOWLEDGE_EXTRACTION_FAILED",
                message: "Document text extraction failed.",
            },
            AppError::KnowledgeRetrievalFailed => Self {
                code: "KNOWLEDGE_RETRIEVAL_FAILED",
                message: "Knowledge retrieval failed.",
            },
            AppError::MemoryWriteDenied => Self {
                code: "MEMORY_WRITE_DENIED",
                message: "The memory write was denied by policy.",
            },
            AppError::MemoryScopeInvalid => Self {
                code: "MEMORY_SCOPE_INVALID",
                message: "The memory scope is not valid.",
            },
            AppError::PlanInvalid => Self {
                code: "PLAN_INVALID",
                message: "The plan is not valid.",
            },
            AppError::PlanTooLarge => Self {
                code: "PLAN_TOO_LARGE",
                message: "The plan exceeds the allowed bounds.",
            },
            AppError::PlanCapabilityDenied => Self {
                code: "PLAN_CAPABILITY_DENIED",
                message: "The plan requires a capability that is not available.",
            },
            AppError::DelegationDenied => Self {
                code: "DELEGATION_DENIED",
                message: "The delegation was denied.",
            },
            AppError::DelegationLimitExceeded => Self {
                code: "DELEGATION_LIMIT_EXCEEDED",
                message: "The delegation limit was exceeded.",
            },
            AppError::CitationInvalid => Self {
                code: "CITATION_INVALID",
                message: "The citation is not valid.",
            },
            AppError::VerificationInsufficientEvidence => Self {
                code: "VERIFICATION_INSUFFICIENT_EVIDENCE",
                message: "There is not enough evidence to verify.",
            },
            AppError::AuthInvalidCredentials => Self {
                code: "AUTH_INVALID_CREDENTIALS",
                message: "The email or password is not correct.",
            },
            AppError::AuthLocked => Self {
                code: "AUTH_LOCKED",
                message: "Too many attempts. Try again shortly.",
            },
            AppError::BootstrapUnavailable => Self {
                code: "BOOTSTRAP_UNAVAILABLE",
                message: "Production setup is not available in the current state.",
            },
            AppError::BootstrapInvalid => Self {
                code: "BOOTSTRAP_INVALID",
                message: "The setup input does not meet the required rules.",
            },
            // الدولة والقطاع حقول يعلنها المستخدم: كل رفض يحمل رمزه الخاص
            // ليعرف الموظف الحقل الذي يجب تصحيحه بالضبط.
            AppError::BootstrapCountryInvalid => Self {
                code: "BOOTSTRAP_COUNTRY_INVALID",
                message: "The country must be selected from the supported ISO 3166-1 alpha-2 codes.",
            },
            AppError::BootstrapIndustryInvalid => Self {
                code: "BOOTSTRAP_INDUSTRY_INVALID",
                message: "The industry name does not meet the input rules.",
            },
            AppError::SelfModificationForbidden => Self {
                code: "SELF_MODIFICATION_FORBIDDEN",
                message: "You cannot change your own account authority.",
            },
            AppError::BackupInvalid => Self {
                code: "BACKUP_INVALID",
                message: "The backup file is not valid.",
            },
            AppError::BackupCorrupted => Self {
                code: "BACKUP_CORRUPTED",
                message: "The backup is corrupted or has been modified.",
            },
            AppError::BackupIncompatible => Self {
                code: "BACKUP_INCOMPATIBLE",
                message: "The backup schema version is not compatible.",
            },
            AppError::BackupStorageError => Self {
                code: "BACKUP_STORAGE_ERROR",
                message: "The backup operation failed on local storage.",
            },
            // سياسة الوجهة: الرمز الثابت للسياسة يُمرَّر كما هو (لا عنوان ولا IP).
            AppError::ProviderEndpointRejected(code) => Self {
                code,
                message: "The provider endpoint was rejected by the network policy.",
            },
            AppError::ProviderNotConfigured => Self {
                code: "RESOURCE_NOT_FOUND",
                message: "The requested provider is not configured in this workspace.",
            },
            AppError::ProviderDisabled => Self {
                code: "PROVIDER_UNAVAILABLE",
                message: "The selected provider is disabled.",
            },
            AppError::ProviderCredentialMissing => Self {
                code: "PROVIDER_UNAVAILABLE",
                message: "No stored credential is available for this provider.",
            },
            AppError::ProviderCredentialStorageUnavailable => Self {
                code: "STORAGE_ERROR",
                message: "The operating system credential store is unavailable.",
            },
            AppError::ProviderModelSelectionInvalid => Self {
                code: "MODEL_UNAVAILABLE",
                message: "The selected model is not available from the selected provider.",
            },
            // وجهة التصدير قرار تحقق صريح؛ فشل الكتابة قرار تخزين منفصل.
            AppError::ReportExportRejected => Self {
                code: "VALIDATION_FAILED",
                message: "The export destination was rejected by the local safety rules.",
            },
            AppError::ReportExportFailed => Self {
                code: "STORAGE_ERROR",
                message: "The report file could not be written to local storage.",
            },
            // 0025: شروط دورة حياة المهمة لها رمزان مستقلان حتى يفهم الموظف
            // سبب الرفض بدقة بدل رسالة عامة لا تدله على الخطوة الناقصة.
            AppError::TaskEvidenceRequired => Self {
                code: "TASK_EVIDENCE_REQUIRED",
                message: "This task cannot be submitted for review before evidence is registered on its obligation.",
            },
            AppError::TaskAssignmentInvalid => Self {
                code: "TASK_ASSIGNMENT_INVALID",
                message: "The selected assignee is not an active member of this organization.",
            },
            // التحقق البشري بلا دليل ادعاء بلا سند — رفض صريح ومعنون.
            AppError::ObligationVerificationRequiresEvidence => Self {
                code: "OBLIGATION_EVIDENCE_REQUIRED",
                message: "Human verification requires at least one registered evidence item on the obligation.",
            },
            // 0026: رموز مستقلة لمسار المستخدمين وكلمات المرور وفصل الأدلة،
            // حتى تُترجم الواجهة سبب الرفض بدقة بدل رسالة عامة.
            AppError::UserEmailExists => Self {
                code: "USER_EMAIL_EXISTS",
                message: "This email address is already used by another account in this organization.",
            },
            AppError::UserRoleNotGrantable => Self {
                code: "USER_ROLE_NOT_GRANTABLE",
                message: "The selected role grants permissions beyond your own authority.",
            },
            AppError::UserRoleUnknown => Self {
                code: "USER_ROLE_UNKNOWN",
                message: "The selected role is not part of this organization's role catalogue.",
            },
            AppError::CurrentPasswordInvalid => Self {
                code: "CURRENT_PASSWORD_INVALID",
                message: "The current password is not correct.",
            },
            AppError::PasswordPolicyViolation => Self {
                code: "PASSWORD_POLICY_VIOLATION",
                message: "The new password does not meet the password policy.",
            },
            AppError::EvidenceSelfVerificationForbidden => Self {
                code: "EVIDENCE_SELF_VERIFICATION_FORBIDDEN",
                message: "The person who registered the evidence cannot decide its verification status.",
            },
            AppError::RegulatorySourceInvalid => Self {
                code: "REGULATORY_SOURCE_INVALID",
                message: "The regulatory source input does not meet the required rules.",
            },
            AppError::RegulatorySourceDuplicate => Self {
                code: "REGULATORY_SOURCE_DUPLICATE",
                message: "A regulatory source with the same authority, document and version already exists.",
            },
            AppError::ObligationSourceInvalid => Self {
                code: "OBLIGATION_SOURCE_INVALID",
                message: "The referenced regulatory source is not available in this workspace.",
            },
            AppError::LastAdministratorProtected => Self {
                code: "LAST_ADMINISTRATOR_PROTECTED",
                message: "The change would leave the organization without an active administrator.",
            },
            AppError::EvidenceExpired => Self {
                code: "EVIDENCE_EXPIRED",
                message: "The evidence has expired and cannot be approved.",
            },
        }
    }
}

impl From<ProviderFailure> for AppError {
    fn from(error: ProviderFailure) -> Self {
        match error {
            ProviderFailure::Unavailable => Self::ProviderUnavailable,
            ProviderFailure::Timeout => Self::ProviderTimeout,
            ProviderFailure::ModelUnavailable => Self::ModelUnavailable,
            ProviderFailure::InputTooLarge => Self::ValidationFailed,
            ProviderFailure::InvalidResponse
            | ProviderFailure::OutputTooLarge
            | ProviderFailure::ToolCallUnsupported => Self::ProviderResponseInvalid,
            ProviderFailure::EndpointRejected(code) => Self::ProviderEndpointRejected(code),
            ProviderFailure::CredentialMissing => Self::ProviderCredentialMissing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, SafeError};

    #[test]
    fn exposes_stable_safe_error_codes_for_obligation_operations() {
        assert_eq!(
            SafeError::from(AppError::AuthorizationDenied).code,
            "AUTHORIZATION_DENIED"
        );
        assert_eq!(
            SafeError::from(AppError::ValidationFailed).code,
            "VALIDATION_FAILED"
        );
        assert_eq!(
            SafeError::from(AppError::ResourceNotFound).code,
            "RESOURCE_NOT_FOUND"
        );
        assert_eq!(SafeError::from(AppError::Conflict).code, "CONFLICT");
        assert_eq!(
            SafeError::from(AppError::InvalidStateTransition).code,
            "INVALID_STATE_TRANSITION"
        );
        assert_eq!(
            SafeError::from(AppError::StorageOperation).code,
            "STORAGE_ERROR"
        );
        assert_eq!(
            SafeError::from(AppError::AgentDisabled).code,
            "AGENT_DISABLED"
        );
        assert_eq!(
            SafeError::from(AppError::SelfApprovalForbidden).code,
            "SELF_APPROVAL_FORBIDDEN"
        );
        assert_eq!(
            SafeError::from(AppError::StaleApproval).code,
            "STALE_APPROVAL"
        );
        assert_eq!(
            SafeError::from(AppError::ProviderUnavailable).code,
            "PROVIDER_UNAVAILABLE"
        );
        assert_eq!(
            SafeError::from(AppError::ProviderTimeout).code,
            "PROVIDER_TIMEOUT"
        );
        assert_eq!(
            SafeError::from(AppError::ProviderResponseInvalid).code,
            "PROVIDER_RESPONSE_INVALID"
        );
        assert_eq!(
            SafeError::from(AppError::ModelUnavailable).code,
            "MODEL_UNAVAILABLE"
        );
        assert_eq!(
            SafeError::from(AppError::ExecutionNotConfigured).code,
            "EXECUTION_NOT_CONFIGURED"
        );
    }

    // GAP-05-OPEN: رمزا رفض بيانات المؤسسة يجب أن يبقى لهما رمز آمن ثابت
    // مستقل عن الرمز العام للتهيئة، حتى تُترجم الرسالة في الواجهة بدقة.
    #[test]
    fn exposes_stable_safe_error_codes_for_organization_profile_rejection() {
        assert_eq!(
            SafeError::from(AppError::BootstrapCountryInvalid).code,
            "BOOTSTRAP_COUNTRY_INVALID"
        );
        assert_eq!(
            SafeError::from(AppError::BootstrapIndustryInvalid).code,
            "BOOTSTRAP_INDUSTRY_INVALID"
        );
    }

    #[test]
    fn exposes_stable_safe_error_codes_for_intelligence_operations() {
        for (error, expected) in [
            (AppError::KnowledgeSourceInvalid, "KNOWLEDGE_SOURCE_INVALID"),
            (
                AppError::KnowledgeSourceTooLarge,
                "KNOWLEDGE_SOURCE_TOO_LARGE",
            ),
            (AppError::KnowledgeDuplicate, "KNOWLEDGE_DUPLICATE"),
            (
                AppError::KnowledgeExtractionFailed,
                "KNOWLEDGE_EXTRACTION_FAILED",
            ),
            (
                AppError::KnowledgeRetrievalFailed,
                "KNOWLEDGE_RETRIEVAL_FAILED",
            ),
            (AppError::MemoryWriteDenied, "MEMORY_WRITE_DENIED"),
            (AppError::MemoryScopeInvalid, "MEMORY_SCOPE_INVALID"),
            (AppError::PlanInvalid, "PLAN_INVALID"),
            (AppError::PlanTooLarge, "PLAN_TOO_LARGE"),
            (AppError::PlanCapabilityDenied, "PLAN_CAPABILITY_DENIED"),
            (AppError::DelegationDenied, "DELEGATION_DENIED"),
            (
                AppError::DelegationLimitExceeded,
                "DELEGATION_LIMIT_EXCEEDED",
            ),
            (AppError::CitationInvalid, "CITATION_INVALID"),
            (
                AppError::VerificationInsufficientEvidence,
                "VERIFICATION_INSUFFICIENT_EVIDENCE",
            ),
        ] {
            assert_eq!(SafeError::from(error).code, expected);
        }
    }
}
