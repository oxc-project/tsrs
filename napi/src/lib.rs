//! Node bindings for the scoped CLI checking driver. JavaScript never receives compiler pointers.

mod system;

use napi::bindgen_prelude::{AsyncTask, Env, Task};
use napi_derive::napi;

/// Native result; the public facade supplies the shared diagnostic declarations.
#[napi(object)]
pub struct CheckResult {
    pub exit_code: i32,
    #[napi(ts_type = "unknown[]")]
    pub diagnostics: Vec<serde_json::Value>,
    pub stdout: String,
    pub stderr: String,
}

fn run(cwd: String, project: String) -> napi::Result<CheckResult> {
    std::thread::Builder::new()
        .name("tsrs-napi".into())
        .stack_size(tsrs_compiler::CHECKER_STACK_SIZE)
        .spawn(move || system::check(&cwd, &project))
        .map_err(|error| napi::Error::from_reason(error.to_string()))?
        .join()
        .map_err(|payload| {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("native compiler panicked");
            napi::Error::from_reason(message)
        })?
        .map_err(napi::Error::from_reason)
}

#[napi]
pub fn check_sync_native(cwd: String, project: String) -> napi::Result<CheckResult> {
    run(cwd, project)
}

pub struct CheckTask {
    input: Option<(String, String)>,
}

impl Task for CheckTask {
    type Output = CheckResult;
    type JsValue = CheckResult;

    fn compute(&mut self) -> napi::Result<Self::Output> {
        let (cwd, project) =
            self.input.take().ok_or_else(|| napi::Error::from_reason("check already completed"))?;
        run(cwd, project)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
        Ok(output)
    }
}

#[napi]
pub fn check_native(cwd: String, project: String) -> AsyncTask<CheckTask> {
    AsyncTask::new(CheckTask { input: Some((cwd, project)) })
}
