use crate::app::apply_orchestrator::run_apply_pipeline;
use crate::app::generate_orchestrator::run_generate_pipeline;
use crate::infra::app_error::AppError;

pub fn run_pipeline() -> Result<(), AppError> {
    if std::env::var("ETB_UI_INPUT").is_ok() {
        run_apply_pipeline()
    } else {
        run_generate_pipeline().map(|_| ())
    }
}