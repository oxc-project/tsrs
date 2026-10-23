use tsrs_contentmapper::{
    METHOD_INITIALIZE, METHOD_TRANSFORM, MappedOutput, ProtocolJson, SupplementalOutput,
    TransformParams, TransformResult,
};
use tsrs_core::json::Value;
use tsrs_ipc as ipc;

use crate::protocol::{
    TestHandler, identity_mapped_output, initialize_result, no_notifications, unexpected_method,
    unmarshal_params,
};

// supplemental.go:11
pub struct SupplementalHandler;

impl TestHandler for SupplementalHandler {}

impl ipc::Handler for SupplementalHandler {
    // supplemental.go:13
    fn handle_request(&self, method: &str, params: Option<&Value>) -> Result<Value, ipc::Error> {
        match method {
            METHOD_INITIALIZE => Ok(initialize_result("mapper").marshal_json()),
            METHOD_TRANSFORM => {
                let p: TransformParams = unmarshal_params(params)?;
                let mapped_output = identity_mapped_output(&p.content)?;
                Ok(TransformResult {
                    mapped_output: MappedOutput {
                        text: "export {};".to_string(),
                        extension: ".ts".to_string(),
                        ..Default::default()
                    },
                    supplemental: vec![SupplementalOutput { mapped_output }],
                    ..Default::default()
                }
                .marshal_json())
            }
            _ => Err(unexpected_method(method)),
        }
    }

    fn handle_notification(
        &self,
        _method: &str,
        _params: Option<&Value>,
    ) -> Result<(), ipc::Error> {
        no_notifications()
    }
}
