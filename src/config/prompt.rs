use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::default::Default;
use std::fmt::Debug;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::config::{api::Api, resolve_config_path};

const PROMPT_FILE: &str = "prompts.toml";
const CONVERSATION_FILE: &str = "conversation.toml";
const CONVERSATIONS_PATH: &str = "saved_conversations";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Prompt {
    pub api: Api,
    pub model: Option<String>,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub char_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>, // unsuported for now
}

impl Default for Prompt {
    /// default to openai and gpt 4 with a preset message telling the
    /// model to behave like smart version of cat.
    fn default() -> Self {
        let messages = vec![
            Message::system( "\
                You are an extremely skilled programmer with a keen eye for detail and an emphasis on readable code. \
                You have been tasked with acting as a smart version of the cat unix program. You take text and a prompt in and write text out. \
                For that reason, it is of crucial importance to just write the desired output. Do not under any circumstance write any comment or thought \
                as your output will be piped into other programs. Do not write the markdown delimiters for code as well. \
                Sometimes you will be asked to implement or extend some input code. Same thing goes here, write only what was asked because what you write will \
                be directly added to the user's editor. \
                Never ever write ``` around the code. \
                Make sure to keep the indentation and formatting. \
            ")
        ];
        Prompt {
            api: Api::Ollama,
            model: None,
            temperature: None,
            messages,
            stream: None,
            char_limit: Some(50000),
        }
    }
}

impl Prompt {
    pub fn empty() -> Self {
        let default_prompt = Prompt::default();
        Prompt {
            api: default_prompt.api,
            model: default_prompt.model,
            temperature: None,
            messages: vec![],
            stream: None,
            char_limit: Some(50000),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: &str) -> Message {
        Message {
            role: "user".to_string(),
            content: content.to_string(),
        }
    }
    pub fn system(content: &str) -> Message {
        Message {
            role: "system".to_string(),
            content: content.to_string(),
        }
    }
    pub fn assistant(content: &str) -> Message {
        Message {
            role: "assistant".to_string(),
            content: content.to_string(),
        }
    }
}

pub(super) fn prompts_path() -> PathBuf {
    resolve_config_path().join(PROMPT_FILE)
}

pub fn conversation_file_path() -> PathBuf {
    resolve_config_path().join(CONVERSATION_FILE)
}

// Get the path to the conversations directory
pub fn conversations_path() -> PathBuf {
    resolve_config_path().join(CONVERSATIONS_PATH)
}

// Get the path to a specific conversation file
pub fn named_conversation_path(name: &str) -> PathBuf {
    conversations_path().join(format!("{}.toml", name))
}

// Get the last conversation as a prompt, if it exists
pub fn get_last_conversation_as_prompt(name: Option<&str>) -> Option<Prompt> {
    if let Some(name) = name {
        let named_path = named_conversation_path(name);
        if !named_path.exists() {
            return None;
        }
        let content = fs::read_to_string(named_path)
            .unwrap_or_else(|error| {
                panic!(
                    "Could not read file {:?}, {:?}",
                    named_conversation_path(name),
                    error
                )
            });
        Some(toml::from_str(&content).expect("failed to load the conversation file"))
    } else {
        let path = conversation_file_path();
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(path)
            .unwrap_or_else(|error| {
                panic!(
                    "Could not read file {:?}, {:?}",
                    conversation_file_path(),
                    error
                )
            });
        Some(toml::from_str(&content).expect("failed to load the conversation file"))
    }
}

pub fn save_conversation(prompt: &Prompt, name: Option<&str>) -> std::io::Result<()> {
    let toml_string = toml::to_string(prompt).expect("Failed to serialize prompt");

    // Always save to conversation.toml
    fs::write(conversation_file_path(), &toml_string)?;

    // If name is provided, also save to named conversation file
    if let Some(name) = name {
        fs::create_dir_all(conversations_path())?;
        fs::write(named_conversation_path(name), &toml_string)?;
    }

    Ok(())
}

pub(super) fn generate_prompts_file() -> std::io::Result<()> {
    let mut prompt_config = HashMap::new();
    prompt_config.insert("default", Prompt::default());
    prompt_config.insert("empty", Prompt::empty());

    std::fs::create_dir_all(prompts_path().parent().unwrap())?;

    let mut prompts_file = fs::File::create(prompts_path())?;

    let doc = "\
        # Prompt config files\n\
        # more details and examples at https://github.com/efugier/smartcat#configuration\n\n";
    prompts_file.write_all(doc.as_bytes())?;

    let prompt_str = toml::to_string_pretty(&prompt_config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    prompts_file.write_all(prompt_str.as_bytes())?;
    Ok(())
}

pub fn get_prompts() -> HashMap<String, Prompt> {
    let content = fs::read_to_string(prompts_path())
        .unwrap_or_else(|error| panic!("Could not read file {:?}, {:?}", prompts_path(), error));
    toml::from_str(&content).expect("could not parse prompt file content")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use crate::config::prompt::Prompt;
    use serial_test::serial;

    fn setup() -> tempfile::TempDir {
        let temp_dir = tempdir().unwrap();
        std::env::set_var("SMARTCAT_CONFIG_PATH", temp_dir.path());
        temp_dir
    }

    fn create_test_prompt() -> Prompt {
        let mut prompt = Prompt::default();
        prompt.messages = vec![(Message::user("test"))];
        prompt
    }

    #[test]
    #[serial]
    fn test_get_and_save_default_conversation() {
        let _temp_dir = setup();
        let test_prompt = create_test_prompt();

        // Test saving conversation
        save_conversation(&test_prompt, None).unwrap();
        assert!(conversation_file_path().exists());

        // Test retrieving conversation
        let loaded_prompt = get_last_conversation_as_prompt(None).unwrap();
        assert_eq!(loaded_prompt, test_prompt);
    }

    #[test]
    #[serial]
    fn test_get_and_save_named_conversation() {
        let _temp_dir = setup();
        let test_prompt = create_test_prompt();
        let conv_name = "test_conversation";

        // Test saving named conversation
        save_conversation(&test_prompt, Some(conv_name)).unwrap();
        assert!(named_conversation_path(conv_name).exists());
        assert!(conversation_file_path().exists()); // Should also save to default location

        // Test retrieving named conversation
        let loaded_prompt = get_last_conversation_as_prompt(Some(conv_name)).unwrap();
        assert_eq!(loaded_prompt, test_prompt);
    }

    #[test]
    #[serial]
    fn test_nonexistent_conversation() {
        let _temp_dir = setup();

        // Test getting nonexistent default conversation
        assert!(get_last_conversation_as_prompt(None).is_none());

        // Test getting nonexistent named conversation
        assert!(get_last_conversation_as_prompt(Some("nonexistent")).is_none());
    }

    #[test]
    #[serial]
    fn test_conversation_file_contents() {
        let _temp_dir = setup();
        let test_prompt = create_test_prompt();
        let conv_name = "test_conversation";

        // Save conversation
        save_conversation(&test_prompt, Some(conv_name)).unwrap();

        // Verify default and named files have identical content
        let default_content = fs::read_to_string(conversation_file_path()).unwrap();
        let named_content = fs::read_to_string(named_conversation_path(conv_name)).unwrap();
        assert_eq!(default_content, named_content);

        // Verify content can be parsed back to original prompt
        let parsed_prompt: Prompt = toml::from_str(&default_content).unwrap();
        assert_eq!(parsed_prompt, test_prompt);
    }

    #[test]
    #[serial]
    fn test_generate_prompts_file() {
        let _temp_dir = setup();

        // Test file generation
        generate_prompts_file().unwrap();
        assert!(prompts_path().exists());

        // Verify file is valid TOML and contains expected content
        let content = fs::read_to_string(prompts_path()).unwrap();
        let prompts: HashMap<String, Prompt> = toml::from_str(&content).unwrap();
        assert!(!prompts.is_empty());
    }

    #[test]
    #[serial]
    fn test_get_prompts() {
        let _temp_dir = setup();

        // Generate prompts file
        generate_prompts_file().unwrap();

        // Test loading prompts
        let prompts = get_prompts();
        assert!(!prompts.is_empty());

        // Verify at least one default prompt exists
        assert!(prompts.contains_key("default"));
    }
}
