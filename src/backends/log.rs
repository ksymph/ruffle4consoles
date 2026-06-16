use ruffle_core::backend::log::LogBackend;

#[derive(Clone)]
pub struct ConsoleLogBackend {}

impl Default for ConsoleLogBackend {
    fn default() -> Self {
        Self {}
    }
}

impl LogBackend for ConsoleLogBackend {
    fn avm_trace(&self, message: &str) {
        tracing::info!(target: "avm_trace", "[AVM] {}", message);
    }

    fn avm_warning(&self, message: &str) {
        tracing::warn!(target: "avm_trace", "[AVM Warning] {}", message);
    }
}