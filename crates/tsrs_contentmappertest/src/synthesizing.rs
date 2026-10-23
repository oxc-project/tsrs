use tsrs_contentmapper::{
    METHOD_INITIALIZE, METHOD_TRANSFORM, MappedOutput, ProtocolJson, TransformParams,
    TransformResult,
};
use tsrs_core::json::Value;
use tsrs_ipc as ipc;

use crate::protocol::{
    TestHandler, initialize_result, marshal_span_map, no_notifications, unexpected_method,
    unmarshal_params,
};

// synthesizing.go:12
const SYNTHESIZED_OUTPUT: &str = "export const el = jsxRuntime(Widget);\n";

// synthesizing.go:14
pub struct SynthesizingHandler;

impl TestHandler for SynthesizingHandler {}

impl ipc::Handler for SynthesizingHandler {
    // synthesizing.go:16
    fn handle_request(&self, method: &str, params: Option<&Value>) -> Result<Value, ipc::Error> {
        match method {
            METHOD_INITIALIZE => Ok(initialize_result("mapper").marshal_json()),
            METHOD_TRANSFORM => {
                let _p: TransformParams = unmarshal_params(params)?;
                let mappings = marshal_span_map(&[])?;
                Ok(TransformResult {
                    mapped_output: MappedOutput {
                        text: SYNTHESIZED_OUTPUT.to_string(),
                        extension: ".ts".to_string(),
                        mappings: Some(mappings),
                        ..Default::default()
                    },
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
