use crate::{
    application::provider_runtime_service::ProviderResolver,
    domain::{
        agent_executor::AgentExecutor,
        error::AppError,
        execution_input::ExecutionInput,
        execution_result::{ExecutionFailureCode, ExecutionResult},
        provider::{ModelProvider, ProviderId},
    },
    infrastructure::database::AppState,
};

pub struct ProviderAgentExecutor<'a> {
    state: &'a AppState,
    provider: &'a dyn ModelProvider,
}

impl<'a> ProviderAgentExecutor<'a> {
    pub fn new(state: &'a AppState, provider: &'a dyn ModelProvider) -> Self {
        Self { state, provider }
    }
}

impl AgentExecutor for ProviderAgentExecutor<'_> {
    fn provider_id(&self) -> ProviderId {
        self.provider.provider_id()
    }

    fn execute(&self, input: ExecutionInput) -> Result<ExecutionResult, AppError> {
        let provider_id = self.provider.provider_id();
        let model_id =
            match ProviderResolver::resolve_enabled_local_model(self.state, self.provider) {
                Ok(model_id) => model_id,
                Err(AppError::ModelUnavailable) => {
                    return Ok(ExecutionResult::failed(
                        provider_id,
                        ExecutionFailureCode::ModelUnavailable,
                    ));
                }
                Err(error) => return Err(error),
            };
        let runtime_input = input
            .instruction()
            .as_runtime_input()
            .map_err(AppError::from)?;

        match self
            .provider
            .generate_non_streaming(&model_id, &runtime_input)
        {
            Ok(generation) => ExecutionResult::completed(provider_id, generation),
            Err(failure) => Ok(ExecutionResult::failed(
                provider_id,
                ExecutionFailureCode::from_provider_failure(failure),
            )),
        }
    }
}
