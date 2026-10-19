use rusqlite::Connection;
use serde::Serialize;

use crate::infrastructure::{obligation_repository::ObligationRepository, risk_repository};

// سجل المحوّلات الموثوق — مرجعه الكود الخلفي حصراً (§33/§34): لا يستطيع
// المستأجر ولا الواجهة ولا النموذج تعريف محوّل أو تزوير مفتاحه. أي مفتاح
// غير معروف هنا يعني أداة غير قابلة للتنفيذ مهما قيل في السجل.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrustedAdapterDefinition {
    pub key: &'static str,
    /// الصلاحية الأساسية للمنتج التي يجب أن يمتلكها الفاعل البشري المطلوب.
    pub required_resource: &'static str,
    pub required_action: &'static str,
}

pub const OBLIGATIONS_BOUNDED_LIST: TrustedAdapterDefinition = TrustedAdapterDefinition {
    key: "nexus.obligations.bounded_list.v1",
    required_resource: "OBLIGATION",
    required_action: "READ",
};

pub const RISKS_BOUNDED_LIST: TrustedAdapterDefinition = TrustedAdapterDefinition {
    key: "nexus.risks.bounded_list.v1",
    required_resource: "RISK",
    required_action: "READ",
};

const KNOWN_ADAPTERS: [TrustedAdapterDefinition; 2] =
    [OBLIGATIONS_BOUNDED_LIST, RISKS_BOUNDED_LIST];

pub fn lookup_adapter(adapter_key: &str) -> Option<TrustedAdapterDefinition> {
    KNOWN_ADAPTERS
        .iter()
        .copied()
        .find(|definition| definition.key == adapter_key)
}

/// فشل محوّل بكود مغلق — لا تفاصيل نقل أو SQL تتجاوز الحدود.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAdapterFailure {
    InputInvalid,
    ExecutionFailed,
    OutputTooLarge,
}

impl ToolAdapterFailure {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::InputInvalid => "TOOL_INPUT_INVALID",
            Self::ExecutionFailed => "TOOL_EXECUTION_FAILED",
            Self::OutputTooLarge => "TOOL_RESULT_TOO_LARGE",
        }
    }
}

// تنفيذ المحوّل على حالة التطبيق مباشرة — قراءة مقيّدة عبر حدود المستودعات
// القائمة (§35) بلا أي وصول كتابة ولا امتداد مستقبلي هنا.
pub fn execute_trusted_adapter(
    state: &crate::infrastructure::database::AppState,
    adapter_key: &str,
    canonical_input: &crate::domain::tool_invocation::CanonicalToolInput,
) -> Result<String, ToolAdapterFailure> {
    let definition = lookup_adapter(adapter_key).ok_or(ToolAdapterFailure::ExecutionFailed)?;
    let arguments: serde_json::Value = serde_json::from_str(canonical_input.as_json())
        .map_err(|_| ToolAdapterFailure::InputInvalid)?;

    let query = arguments
        .get("query")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let limit = match arguments.get("limit") {
        None => 5_u64,
        Some(value) => {
            let raw = value.as_u64().ok_or(ToolAdapterFailure::InputInvalid)?;
            if !(1..=10).contains(&raw) {
                return Err(ToolAdapterFailure::InputInvalid);
            }
            raw
        }
    };

    let connection = state
        .connection
        .lock()
        .map_err(|_| ToolAdapterFailure::ExecutionFailed)?;
    let output = if definition.key == OBLIGATIONS_BOUNDED_LIST.key {
        bounded_obligations_output(&connection, &state.tenant_id(), query.as_deref(), limit)?
    } else if definition.key == RISKS_BOUNDED_LIST.key {
        bounded_risks_output(&connection, &state.tenant_id(), query.as_deref(), limit)?
    } else {
        return Err(ToolAdapterFailure::ExecutionFailed);
    };

    crate::domain::tool_invocation::validate_bounded_tool_output(&output)
        .map_err(|_| ToolAdapterFailure::OutputTooLarge)?;
    Ok(output)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoundedObligationItem {
    id: String,
    title: String,
    status: crate::domain::models::ObligationStatus,
    priority: crate::domain::models::ObligationPriority,
    compliance_status: crate::domain::models::ComplianceStatus,
    due_date: Option<String>,
}

fn bounded_obligations_output(
    connection: &Connection,
    tenant_id: &str,
    query: Option<&str>,
    limit: u64,
) -> Result<String, ToolAdapterFailure> {
    let items = ObligationRepository::list_active(connection, tenant_id)
        .map_err(|_| ToolAdapterFailure::ExecutionFailed)?;
    let filtered = items
        .into_iter()
        .filter(|item| match query {
            Some(term) => item.title.to_lowercase().contains(&term.to_lowercase()),
            None => true,
        })
        .take(limit as usize)
        .map(|item| BoundedObligationItem {
            id: item.id,
            title: item.title,
            status: item.status,
            priority: item.priority,
            compliance_status: item.compliance_status,
            due_date: item.due_date,
        })
        .collect::<Vec<_>>();
    let total = filtered.len() as u64;
    serde_json::to_string(&serde_json::json!({ "items": filtered, "total": total }))
        .map_err(|_| ToolAdapterFailure::ExecutionFailed)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BoundedRiskItem {
    id: String,
    title: String,
    severity: crate::domain::models::RiskSeverity,
    status: crate::domain::models::RiskStatus,
}

fn bounded_risks_output(
    connection: &Connection,
    tenant_id: &str,
    query: Option<&str>,
    limit: u64,
) -> Result<String, ToolAdapterFailure> {
    let risks = risk_repository::load_risks(connection, tenant_id, "")
        .map_err(|_| ToolAdapterFailure::ExecutionFailed)?;
    let filtered = risks
        .items
        .into_iter()
        .filter(|item| match query {
            Some(term) => item.title.to_lowercase().contains(&term.to_lowercase()),
            None => true,
        })
        .take(limit as usize)
        .map(|item| BoundedRiskItem {
            id: item.id,
            title: item.title,
            severity: item.severity,
            status: item.status,
        })
        .collect::<Vec<_>>();
    let total = filtered.len() as u64;
    serde_json::to_string(&serde_json::json!({ "items": filtered, "total": total }))
        .map_err(|_| ToolAdapterFailure::ExecutionFailed)
}

#[cfg(test)]
mod tests {
    use super::{execute_trusted_adapter, lookup_adapter, ToolAdapterFailure};
    use crate::domain::tool::ToolContract;
    use crate::domain::tool_invocation::CanonicalToolInput;
    use crate::infrastructure::database::initialize_demo_database;
    use rusqlite::Connection;

    fn list_contract() -> ToolContract {
        ToolContract {
            fields: vec![
                crate::domain::tool::ToolContractField {
                    name: "query".to_owned(),
                    data_type: "string".to_owned(),
                    required: false,
                    description: "filter".to_owned(),
                },
                crate::domain::tool::ToolContractField {
                    name: "limit".to_owned(),
                    data_type: "integer".to_owned(),
                    required: false,
                    description: "bound".to_owned(),
                },
            ],
            allows_additional_fields: false,
        }
    }

    #[test]
    fn unknown_adapter_key_is_rejected_and_known_keys_resolve() {
        assert!(lookup_adapter("nexus.shell.v1").is_none());
        assert!(lookup_adapter("../../obligations").is_none());
        let definition =
            lookup_adapter("nexus.obligations.bounded_list.v1").expect("المحوّل المدمج يجب أن يُعرف");
        assert_eq!(definition.required_resource, "OBLIGATION");
    }

    #[test]
    fn obligations_adapter_returns_bounded_product_read() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل المفاتيح يجب أن ينجح");
        initialize_demo_database(&connection).expect("تهيئة الديمو يجب أن تنجح");
        let state = crate::infrastructure::database::AppState::test_instance(
            connection,
            crate::infrastructure::database::DEMO_TENANT_ID.to_owned(),
            crate::infrastructure::database::DEMO_OPERATOR_USER_ID.to_owned(),
        );

        let input = CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"query": "evidence", "limit": 2}),
            &list_contract(),
        )
        .expect("مدخل الاختبار يجب أن يُقنن");
        let output = execute_trusted_adapter(&state, "nexus.obligations.bounded_list.v1", &input)
            .expect("المحوّل القرائي يجب أن ينجح");
        assert!(output.contains("\"total\":"));
        assert!(output.len() <= 1024);

        let oversized_limit = CanonicalToolInput::validate_and_canonicalize(
            &serde_json::json!({"limit": 11}),
            &list_contract(),
        )
        .expect("المدخل يجب أن يُقنن");
        assert_eq!(
            execute_trusted_adapter(
                &state,
                "nexus.obligations.bounded_list.v1",
                &oversized_limit
            ),
            Err(ToolAdapterFailure::InputInvalid)
        );
    }
}
