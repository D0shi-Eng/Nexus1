use serde::Serialize;

use crate::{domain::error::AppError, infrastructure::database::AppState};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModuleAvailability {
    pub key: &'static str,
    pub status: &'static str,
    pub message_key: &'static str,
}

// سجل الحقيقة لوحدات المنتج الـ15 — يجب أن يطابق حالة الوصل الفعلية لا الانطباعات.
// AVAILABLE = مسار منتجي موصول؛ READ_ONLY = بيانات قراءة موثوقة بلا طفرات معرفة؛
// NOT_IMPLEMENTED = النظام غير موجود بعد (مالكه مرحلة رئيسية لاحقة).
const MODULES: [ModuleAvailability; 15] = [
    ModuleAvailability {
        key: "overview",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "attention",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "obligations",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "regulatory",
        status: "READ_ONLY",
        message_key: "moduleAvailability.regulatoryScope",
    },
    ModuleAvailability {
        key: "risks",
        status: "READ_ONLY",
        message_key: "moduleAvailability.risksScope",
    },
    ModuleAvailability {
        key: "evidence",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "tasks",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "controls",
        // الضوابط صارت مساراً منتجياً كاملاً في 0022 (تأليف + ربط + تقييم) لا سجلاً للقراءة فقط.
        // مفتاح الرسالة ما زال controlsScope ويجب أن يُحدَّث نصّه في اللغات على يد مالك الواجهة.
        status: "AVAILABLE",
        message_key: "moduleAvailability.controlsScope",
    },
    ModuleAvailability {
        key: "agents",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "knowledge",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
    ModuleAvailability {
        key: "reports",
        // التقارير صارت مساراً منتجياً حقيقياً: كتالوج + توليد من بيانات فعلية
        // + تصدير ملفات بصلاحية منفصلة (report:read / report:export).
        // مفتاح الرسالة ما زال reportsScope ويجب أن يُحدَّث نصّه في اللغات
        // على يد مالك الواجهة — نفس نمط controlsScope أعلاه.
        status: "AVAILABLE",
        message_key: "moduleAvailability.reportsScope",
    },
    ModuleAvailability {
        key: "audit",
        status: "READ_ONLY",
        message_key: "moduleAvailability.auditScope",
    },
    ModuleAvailability {
        key: "organization",
        status: "READ_ONLY",
        message_key: "moduleAvailability.organizationScope",
    },
    ModuleAvailability {
        key: "integrations",
        status: "READ_ONLY",
        message_key: "moduleAvailability.integrationsScope",
    },
    ModuleAvailability {
        key: "settings",
        status: "AVAILABLE",
        message_key: "moduleAvailability.available",
    },
];

pub fn get_module_availability(_state: &AppState) -> Result<Vec<ModuleAvailability>, AppError> {
    Ok(MODULES.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_each_navigation_module_with_an_explicit_truthful_status() {
        assert_eq!(MODULES.len(), 15);
        assert!(MODULES
            .iter()
            .all(|module| matches!(module.status, "AVAILABLE" | "READ_ONLY" | "NOT_IMPLEMENTED")));
        assert!(MODULES
            .iter()
            .all(|module| module.message_key.starts_with("moduleAvailability.")));
    }

    #[test]
    fn reserves_not_implemented_only_for_future_phase_subsystems() {
        // لا وحدة بقيت غير منفذة: التقارير أُنجزت كسطح منتجي حقيقي، والمعرفة
        // صارت سطحاً منتجياً في MP04. القائمة الفارغة قرار معلن لا سهو.
        let not_implemented: Vec<&str> = MODULES
            .iter()
            .filter(|module| module.status == "NOT_IMPLEMENTED")
            .map(|module| module.key)
            .collect();
        assert!(not_implemented.is_empty());
    }

    // التقارير لم تعد معلنة كغير منفذة — تغيير مقصود ومُعلن لا انزلاق صامت.
    #[test]
    fn reports_are_available_now_that_generation_and_export_exist() {
        let reports = MODULES
            .iter()
            .find(|module| module.key == "reports")
            .expect("وحدة التقارير يجب أن تكون معرّفة");
        assert_eq!(reports.status, "AVAILABLE");
        assert_eq!(reports.message_key, "moduleAvailability.reportsScope");
    }

    // الضوابط لم تعد قراءة فقط بعد 0022 — تغيير مقصود ومُعلن لا انزلاق صامت.
    #[test]
    fn controls_are_available_now_that_authoring_exists() {
        let controls = MODULES
            .iter()
            .find(|module| module.key == "controls")
            .expect("وحدة الضوابط يجب أن تكون معرّفة");
        assert_eq!(controls.status, "AVAILABLE");

        // بقية الوحدات القرائية لم تتغير بالخطأ.
        let read_only: Vec<&str> = MODULES
            .iter()
            .filter(|module| module.status == "READ_ONLY")
            .map(|module| module.key)
            .collect();
        assert_eq!(
            read_only,
            vec![
                "regulatory",
                "risks",
                "audit",
                "organization",
                "integrations"
            ]
        );
    }
}
