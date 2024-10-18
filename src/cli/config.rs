use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Git {
    pub(crate) enable: bool,
    pub(crate) repository_url: Option<String>,
    pub(crate) repository_name: Option<String>,
    pub(crate) disable_push: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FloccusCliConfig {
    pub(crate) git: Git,
}

