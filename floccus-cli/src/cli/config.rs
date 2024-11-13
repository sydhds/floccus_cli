use serde::Deserialize;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Deserialize)]
pub(crate) struct Git {
    pub(crate) enable: bool,
    pub(crate) repository_url: Option<Url>,
    pub(crate) repository_name: Option<String>,
    pub(crate) repository_token: Option<String>,
    pub(crate) repository_ssh_key: Option<PathBuf>,
    pub(crate) disable_push: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FloccusCliConfig {
    pub(crate) git: Git,
}
