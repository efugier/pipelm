use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::default::Default;
use std::fmt::Debug;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::FromStr;

use super::{prompt::Prompt, resolve_config_path};

const API_KEYS_FILE: &str = ".api_configs.toml";

#[derive(clap::ValueEnum, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Api {
    AnotherApiForTests,
    Ollama,
    Anthropic,
    Groq,
    Mistral,
    Openai,
    AzureOpenai,
    Cerebras,
}

impl FromStr for Api {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Api::Ollama),
            "openai" => Ok(Api::Openai),
            "azureopenai" => Ok(Api::AzureOpenai),
            "mistral" => Ok(Api::Mistral),
            "groq" => Ok(Api::Groq),
            "anthropic" => Ok(Api::Anthropic),
            "cerebras" => Ok(Api::Cerebras),
            _ => Err(()),
        }
    }
}

impl ToString for Api {
    fn to_string(&self) -> String {
        match self {
            Api::Ollama => "ollama".to_string(),
            Api::Openai => "openai".to_string(),
            Api::AzureOpenai => "azureopenai".to_string(),
            Api::Mistral => "mistral".to_string(),
            Api::Groq => "groq".to_string(),
            Api::Anthropic => "anthropic".to_string(),
            Api::Cerebras => "cerebras".to_string(),
            v => panic!(
                "{:?} is not implemented, use one among {:?}",
                v,
                vec![Api::Openai]
            ),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ApiConfig {
    pub api_key: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(
        default = "default_timeout_seconds",
        skip_serializing_if = "Option::is_none"
    )]
    pub timeout_seconds: Option<u32>,
}

pub(super) fn default_timeout_seconds() -> Option<u32> {
    Some(180)
}

impl Default for ApiConfig {
    // default to ollama
    fn default() -> Self {
        ApiConfig::ollama()
    }
}

impl ApiConfig {
    pub fn get_api_key(&self) -> String {
        self.api_key
            .clone()
            .or_else(|| {
                self.api_key_command.clone().map(|command| {
                    let output = if cfg!(windows) {
                        std::process::Command::new("cmd")
                            .stdin(Stdio::inherit())
                            .arg("/c")
                            .arg(command)
                            .output()
                    } else {
                        std::process::Command::new("sh")
                            .stdin(Stdio::inherit())
                            .arg("-c")
                            .arg(command)
                            .output()
                    }
                    .expect("Failed to run the api command")
                    .stdout;
                    String::from_utf8(output)
                        .expect("Invalid UTF-8 from command")
                        .trim()
                        .to_string()
                })
            })
            .unwrap_or_default()
    }

    pub(super) fn ollama() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("http://localhost:11434/api/chat"),
            default_model: Some(String::from("phi3")),
            version: None,
            timeout_seconds: Some(180),
        }
    }

    pub(super) fn openai() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("https://api.openai.com/v1/chat/completions"),
            default_model: Some(String::from("gpt-4")),
            version: None,
            timeout_seconds: None,
        }
    }

    pub(super) fn azureopenai() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("https://your-azure-endpoint.azure.com/openai/deployments/your-deployment-id/chat/completions?api-version=2024-06-01"),
            default_model: Some(String::from("gpt-4o")),
            version: None,
            timeout_seconds: None,
        }
    }

    pub(super) fn mistral() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("https://api.mistral.ai/v1/chat/completions"),
            default_model: Some(String::from("mistral-medium")),
            version: None,
            timeout_seconds: None,
        }
    }

    pub(super) fn groq() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("https://api.groq.com/openai/v1/chat/completions"),
            default_model: Some(String::from("llama3-70b-8192")),
            version: None,
            timeout_seconds: None,
        }
    }

    pub(super) fn anthropic() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("https://api.anthropic.com/v1/messages"),
            default_model: Some(String::from("claude-3-opus-20240229")),
            version: Some(String::from("2023-06-01")),
            timeout_seconds: None,
        }
    }

    pub(super) fn cerebras() -> Self {
        ApiConfig {
            api_key_command: None,
            api_key: None,
            url: String::from("https://api.cerebras.ai/v1/chat/completions"),
            default_model: Some(String::from("llama3.1-70b")),
            version: None,
            timeout_seconds: None,
        }
    }
}

pub(super) fn api_keys_path() -> PathBuf {
    resolve_config_path().join(API_KEYS_FILE)
}

pub(super) fn generate_api_keys_file() -> std::io::Result<()> {
    let mut api_config = HashMap::new();
    api_config.insert(Api::Ollama.to_string(), ApiConfig::ollama());
    api_config.insert(Api::Openai.to_string(), ApiConfig::openai());
    api_config.insert(Api::AzureOpenai.to_string(), ApiConfig::azureopenai());
    api_config.insert(Api::Mistral.to_string(), ApiConfig::mistral());
    api_config.insert(Api::Groq.to_string(), ApiConfig::groq());
    api_config.insert(Api::Anthropic.to_string(), ApiConfig::anthropic());
    api_config.insert(Api::Cerebras.to_string(), ApiConfig::cerebras());

    // Default, should override one of the above
    api_config.insert(Prompt::default().api.to_string(), ApiConfig::default());

    std::fs::create_dir_all(api_keys_path().parent().unwrap())?;

    let mut config_file = fs::File::create(api_keys_path())?;

    {
        let api_key_doc = "\
        # Api config files, use `api_key` or `api_key_command` fields\n\
        # to set the api key for each api\n\
        # more details at https://github.com/efugier/smartcat#configuration\n\n";
        config_file.write_all(api_key_doc.as_bytes())?;
    }

    let api_config_str = toml::to_string_pretty(&api_config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    config_file.write_all(api_config_str.as_bytes())?;

    Ok(())
}

pub fn get_api_config(api: &str) -> ApiConfig {
    let content = fs::read_to_string(api_keys_path())
        .unwrap_or_else(|error| panic!("Could not read file {:?}, {:?}", api_keys_path(), error));

    let mut api_configs: HashMap<String, ApiConfig> = toml::from_str(&content).unwrap();

    api_configs.remove(api).unwrap_or_else(|| {
        panic!(
            "Prompt {} not found, availables ones are: {:?}",
            api,
            api_configs.keys().collect::<Vec<_>>()
        )
    })
}
