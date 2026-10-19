use serde::Serialize;

use crate::domain::error::AppError;

// تصنيف الحقيقة الموحد للمرحلة 04 — يُطبق على الذاكرة والنتائج والتوليف.
// الانتقالات محصورة حتمياً: لا ترقية تلقائية من AI إلى الحقيقة الموثقة.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TruthClass {
    Fact,
    Verified,
    Claim,
    Assumption,
    Inference,
    Opinion,
    Unknown,
    Conflicting,
    AiInterpretation,
}

impl TruthClass {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Fact => "FACT",
            Self::Verified => "VERIFIED",
            Self::Claim => "CLAIM",
            Self::Assumption => "ASSUMPTION",
            Self::Inference => "INFERENCE",
            Self::Opinion => "OPINION",
            Self::Unknown => "UNKNOWN",
            Self::Conflicting => "CONFLICTING",
            Self::AiInterpretation => "AI_INTERPRETATION",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "FACT" => Ok(Self::Fact),
            "VERIFIED" => Ok(Self::Verified),
            "CLAIM" => Ok(Self::Claim),
            "ASSUMPTION" => Ok(Self::Assumption),
            "INFERENCE" => Ok(Self::Inference),
            "OPINION" => Ok(Self::Opinion),
            "UNKNOWN" => Ok(Self::Unknown),
            "CONFLICTING" => Ok(Self::Conflicting),
            "AI_INTERPRETATION" => Ok(Self::AiInterpretation),
            _ => Err(AppError::DataAccess),
        }
    }
}

// بوابة الترقية الحتمية: AI_INTERPRETATION لا تصبح VERIFIED مباشرة أبداً،
// وCONFLICTING لا تُحل إلا بقرار صريح، وUNKNOWN تبقى عند نقص الدليل.
// المرجع البشري يعلمنا إن كان هذا الترقية مسموحة (وليس من يطلبها).
// الفاعل الآلي طرف العقد في can_promote ويُختبر حتمياً — لا يُنشئ إنتاجياً
// لأن المنتج لا يملك أي مسار ترقية آلي أصلاً (هذا هو الأصل).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TruthPromotionActor {
    HumanReviewer,
    AiOrAutomated,
}

pub fn can_promote(from: TruthClass, to: TruthClass, actor: TruthPromotionActor) -> bool {
    if from == to {
        return false;
    }
    match (from, to, actor) {
        // الترقية إلى VERIFIED بشرية حصراً — ومن CLAIM فقط (بعد استيفاء البوابة)
        (TruthClass::Claim, TruthClass::Verified, TruthPromotionActor::HumanReviewer) => true,
        // أي مسار آخر إلى VERIFIED ممنوع — خاصة من AI_INTERPRETATION مباشرة
        (_, TruthClass::Verified, _) => false,
        // القفل الحتمي: التصنيفات المغلقة لا تتحول آلياً
        (TruthClass::Conflicting, _, TruthPromotionActor::AiOrAutomated) => false,
        (TruthClass::Unknown, _, TruthPromotionActor::AiOrAutomated) => false,
        (
            TruthClass::AiInterpretation,
            TruthClass::Conflicting,
            TruthPromotionActor::AiOrAutomated,
        ) => true,
        // المسارات المتبقية لا تحدث في المنتج الحالي — مغلقة افتراضياً
        _ => false,
    }
}

// نتيجة التحقق المهيكلة — ليست درجة ثقة مجردة (§57/C-8)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationOutcome {
    Supported,
    PartiallySupported,
    Unsupported,
    Conflicting,
    Unknown,
    InsufficientEvidence,
}

impl VerificationOutcome {
    pub fn as_code(&self) -> &'static str {
        match self {
            Self::Supported => "SUPPORTED",
            Self::PartiallySupported => "PARTIALLY_SUPPORTED",
            Self::Unsupported => "UNSUPPORTED",
            Self::Conflicting => "CONFLICTING",
            Self::Unknown => "UNKNOWN",
            Self::InsufficientEvidence => "INSUFFICIENT_EVIDENCE",
        }
    }

    pub fn from_code(value: &str) -> Result<Self, AppError> {
        match value {
            "SUPPORTED" => Ok(Self::Supported),
            "PARTIALLY_SUPPORTED" => Ok(Self::PartiallySupported),
            "UNSUPPORTED" => Ok(Self::Unsupported),
            "CONFLICTING" => Ok(Self::Conflicting),
            "UNKNOWN" => Ok(Self::Unknown),
            "INSUFFICIENT_EVIDENCE" => Ok(Self::InsufficientEvidence),
            _ => Err(AppError::DataAccess),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{can_promote, TruthClass, TruthPromotionActor, VerificationOutcome};

    #[test]
    fn ai_interpretation_never_promotes_directly_to_verified() {
        assert!(!can_promote(
            TruthClass::AiInterpretation,
            TruthClass::Verified,
            TruthPromotionActor::HumanReviewer
        ));
        assert!(!can_promote(
            TruthClass::AiInterpretation,
            TruthClass::Verified,
            TruthPromotionActor::AiOrAutomated
        ));
    }

    #[test]
    fn verified_requires_human_actor_and_claim_origin() {
        assert!(can_promote(
            TruthClass::Claim,
            TruthClass::Verified,
            TruthPromotionActor::HumanReviewer
        ));
        assert!(!can_promote(
            TruthClass::Claim,
            TruthClass::Verified,
            TruthPromotionActor::AiOrAutomated
        ));
        assert!(!can_promote(
            TruthClass::Fact,
            TruthClass::Verified,
            TruthPromotionActor::HumanReviewer
        ));
    }

    #[test]
    fn conflicting_and_unknown_stay_locked_against_automation() {
        assert!(!can_promote(
            TruthClass::Conflicting,
            TruthClass::Claim,
            TruthPromotionActor::AiOrAutomated
        ));
        assert!(!can_promote(
            TruthClass::Unknown,
            TruthClass::Claim,
            TruthPromotionActor::AiOrAutomated
        ));
    }

    #[test]
    fn codes_round_trip_for_every_truth_and_verification_value() {
        for code in [
            "FACT",
            "VERIFIED",
            "CLAIM",
            "ASSUMPTION",
            "INFERENCE",
            "OPINION",
            "UNKNOWN",
            "CONFLICTING",
            "AI_INTERPRETATION",
        ] {
            assert_eq!(
                TruthClass::from_code(code)
                    .expect("كود الحقيقة يجب أن يُقرأ")
                    .as_code(),
                code
            );
        }
        for code in [
            "SUPPORTED",
            "PARTIALLY_SUPPORTED",
            "UNSUPPORTED",
            "CONFLICTING",
            "UNKNOWN",
            "INSUFFICIENT_EVIDENCE",
        ] {
            assert_eq!(
                VerificationOutcome::from_code(code)
                    .expect("كود التحقق يجب أن يُقرأ")
                    .as_code(),
                code
            );
        }
        assert!(TruthClass::from_code("OFFICIAL").is_err());
    }
}
