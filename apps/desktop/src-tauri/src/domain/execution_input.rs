use crate::domain::provider::{ProviderFailure, RuntimeExecutionInput, MAX_RUNTIME_INPUT_BYTES};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionInstruction {
    value: String,
}

impl ExecutionInstruction {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderFailure> {
        let value = value.into();
        if value.trim().is_empty() || value.as_bytes().len() > MAX_RUNTIME_INPUT_BYTES {
            return Err(ProviderFailure::InputTooLarge);
        }

        Ok(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn as_runtime_input(&self) -> Result<RuntimeExecutionInput, ProviderFailure> {
        RuntimeExecutionInput::new(self.value.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionInput {
    instruction: ExecutionInstruction,
}

impl ExecutionInput {
    pub fn new(instruction: ExecutionInstruction) -> Self {
        Self { instruction }
    }

    pub fn instruction(&self) -> &ExecutionInstruction {
        &self.instruction
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionInput, ExecutionInstruction};
    use crate::domain::provider::{ProviderFailure, MAX_RUNTIME_INPUT_BYTES};

    #[test]
    fn accepts_only_a_non_empty_bounded_instruction() {
        let instruction = ExecutionInstruction::new("trusted instruction")
            .expect("تعليمة الاختبار يجب أن تكون صالحة");
        let input = ExecutionInput::new(instruction);

        assert_eq!(input.instruction().as_str(), "trusted instruction");
        assert_eq!(
            input
                .instruction()
                .as_runtime_input()
                .expect("مدخل المزود يجب أن يبنى")
                .as_str(),
            "trusted instruction"
        );
    }

    #[test]
    fn rejects_empty_or_oversized_instruction() {
        assert!(matches!(
            ExecutionInstruction::new("   "),
            Err(ProviderFailure::InputTooLarge)
        ));
        assert!(matches!(
            ExecutionInstruction::new("a".repeat(MAX_RUNTIME_INPUT_BYTES + 1)),
            Err(ProviderFailure::InputTooLarge)
        ));
    }
}
