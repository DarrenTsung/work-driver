use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Check {
    async fn check(&self) -> Result<Vec<String>>;
}
