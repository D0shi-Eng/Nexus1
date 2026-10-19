use crate::domain::error::AppError;

pub const EXECUTION_INTENT_VERSION: &str = "execution-intent.v1";
const MAX_INTENT_TEXT_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionIntentKind {
    GovernedAgentExecution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionIntent {
    kind: ExecutionIntentKind,
    objective: String,
    constraints: Vec<String>,
}

impl ExecutionIntent {
    pub fn new(
        kind: ExecutionIntentKind,
        objective: impl Into<String>,
        constraints: Vec<String>,
    ) -> Result<Self, AppError> {
        let objective = validate_text(objective.into())?;
        if constraints.len() > 16 {
            return Err(AppError::ValidationFailed);
        }
        let constraints = constraints
            .into_iter()
            .map(validate_text)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            kind,
            objective,
            constraints,
        })
    }

    pub fn kind(&self) -> &ExecutionIntentKind {
        &self.kind
    }

    pub fn objective(&self) -> &str {
        &self.objective
    }

    pub fn constraints(&self) -> &[String] {
        &self.constraints
    }
}

fn validate_text(value: String) -> Result<String, AppError> {
    if value.trim().is_empty() || value.as_bytes().len() > MAX_INTENT_TEXT_BYTES {
        return Err(AppError::ValidationFailed);
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{ExecutionIntent, ExecutionIntentKind, EXECUTION_INTENT_VERSION};

    #[test]
    fn creates_a_small_typed_intent() {
        let intent = ExecutionIntent::new(
            ExecutionIntentKind::GovernedAgentExecution,
            "produce a bounded interpretation",
            vec!["do not execute side effects".to_owned()],
        )
        .expect("يجب إنشاء النية الموثوقة");

        assert_eq!(intent.objective(), "produce a bounded interpretation");
        assert_eq!(EXECUTION_INTENT_VERSION, "execution-intent.v1");
    }

    #[test]
    fn rejects_empty_objective() {
        assert!(
            ExecutionIntent::new(ExecutionIntentKind::GovernedAgentExecution, " ", vec![]).is_err()
        );
    }
}
