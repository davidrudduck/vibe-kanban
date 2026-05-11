#[derive(Clone, Default)]
pub struct BillingService {}

impl BillingService {
    pub fn new() -> Self {
        Self {}
    }

    pub fn is_configured(&self) -> bool {
        false
    }

    pub fn provider(&self) -> Option<std::convert::Infallible> {
        None
    }
}
