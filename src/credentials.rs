use anyhow::{Context, Result};

const SERVICE: &str = "llmfuck";

pub fn store(reference: &str, secret: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, reference)?
        .set_password(secret)
        .context("system credential store rejected the API key")
}

pub fn load(reference: &str) -> Result<String> {
    keyring::Entry::new(SERVICE, reference)?
        .get_password()
        .context("failed to read API key from the system credential store")
}

pub fn delete(reference: &str) -> Result<()> {
    keyring::Entry::new(SERVICE, reference)?
        .delete_credential()
        .context("failed to remove API key from the system credential store")
}
