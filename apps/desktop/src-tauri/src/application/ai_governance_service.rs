use crate::{domain::error::AppError, infrastructure::database::AppState};

pub fn has_enabled_model(state: &AppState) -> Result<bool, AppError> {
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_model_registry WHERE tenant_id = ?1 AND enabled = 1)",
            [&state.tenant_id()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::DataAccess)
}
