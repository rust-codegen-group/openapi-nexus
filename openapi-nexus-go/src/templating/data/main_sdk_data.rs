//! Main SDK data for template rendering

use serde::{Deserialize, Serialize};

use crate::templating::data::CommonFileHeaderData;

/// Sub-client information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubClientInfo {
    pub name: String,
    pub type_name: String,
}

/// SDK option function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkOption {
    pub name: String,
    pub param_type: String,
    pub description: Option<String>,
}

/// Main SDK data for template rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainSdkData {
    pub sdk_name: String,
    pub package_name: String,
    pub sub_clients: Vec<SubClientInfo>,
    pub sdk_options: Vec<SdkOption>,
    pub common_file_header: CommonFileHeaderData,
}

impl MainSdkData {
    pub fn new(
        sdk_name: String,
        package_name: String,
        common_file_header: CommonFileHeaderData,
    ) -> Self {
        Self {
            sdk_name,
            package_name,
            sub_clients: Vec::new(),
            sdk_options: Vec::new(),
            common_file_header,
        }
    }

    pub fn with_sub_clients(mut self, sub_clients: Vec<SubClientInfo>) -> Self {
        self.sub_clients = sub_clients;
        self
    }

    pub fn with_sdk_options(mut self, sdk_options: Vec<SdkOption>) -> Self {
        self.sdk_options = sdk_options;
        self
    }
}
