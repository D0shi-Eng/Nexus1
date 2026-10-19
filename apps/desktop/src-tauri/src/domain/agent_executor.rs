use crate::domain::{
    error::AppError, execution_input::ExecutionInput, execution_result::ExecutionResult,
    provider::ProviderId,
};

pub trait AgentExecutor {
    fn provider_id(&self) -> ProviderId;
    fn execute(&self, input: ExecutionInput) -> Result<ExecutionResult, AppError>;
}
