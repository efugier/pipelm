mod config;
mod prompt_customization;
mod text;
mod utils;

use crate::config::{
    api::Api,
    ensure_config_usable,
    prompt::{get_last_conversation_as_prompt, save_conversation, get_prompts, Prompt},
};
use prompt_customization::customize_prompt;
use crate::utils::valid_conversation_name;

use clap::{Args, Parser};
use log::debug;
use std::io::{self, IsTerminal, Read, Write};

use text::process_input_with_request;

const DEFAULT_PROMPT_NAME: &str = "default";

#[derive(Debug, Parser)]
#[command(
    name = "smartcat (sc)",
    author = "Emilien Fugier",
    version = "2.2.0",
    about = "Putting a brain behind `cat`. CLI interface to bring language models in the Unix ecosystem 🐈‍⬛",
    long_about = None,
    after_help = "Examples:
=========

- sc <my_input>
- sc <my_prompt_template_name> <my_input>

- sc \"say hi\"  # just ask

- sc test                         # use templated prompts
- sc test \"and parametrize them\"  # extend them on the fly

- sc \"explain how to use this program\" -c **/*.md main.py  # use files as context

- git diff | sc \"summarize the changes\"  # pipe data in

- cat en.md | sc \"translate in french\" >> fr.md   # write data out
- sc -e \"use a more informal tone\" -t 2 >> fr.md  # extend the conversation and raise the temprature
"
)]
struct Cli {
    /// ref to a prompt template from config or straight input (will use `default` prompt template if input)
    input_or_template_ref: Option<String>,
    /// if the first arg matches a config template, the second will be used as input
    input_if_template_ref: Option<String>,
    /// whether to extend the previous conversation or start a new one
    #[arg(short, long)]
    extend_conversation: bool,
    /// whether to repeat the input before the output, useful to extend instead of replacing
    #[arg(short, long)]
    repeat_input: bool,
    /// conversation name
    #[arg(short, long, value_parser = valid_conversation_name)]
    name: Option<String>,
    #[command(flatten)]
    prompt_params: PromptParams,
}

#[derive(Debug, Default, Args)]
#[group(id = "prompt_params")]
struct PromptParams {
    /// overrides which api to hit
    #[arg(long)]
    api: Option<Api>,
    /// overrides which model (of the api) to use
    #[arg(short, long)]
    model: Option<String>,
    /// higher temperature  means answer further from the average
    #[arg(short, long)]
    temperature: Option<f32>,
    /// max number of chars to include, ask for user approval if more, 0 = no limit
    #[arg(short = 'l', long)]
    char_limit: Option<u32>,
    /// glob patterns or list of files to use the content as context
    /// make sure it's the last arg.
    #[arg(short, long, num_args= 1.., value_delimiter = ' ', verbatim_doc_comment)]
    context: Vec<String>,
}

fn main() {
    env_logger::init();

    let stdin = io::stdin();
    let mut output = io::stdout();
    let mut input = String::new();

    // case for testing
    // TODO: mock API and actually use the real processing
    if std::env::var("SMARTCAT_TEST").unwrap_or_default() == "1" {
        if let Err(e) = stdin
            .lock()
            .read_to_string(&mut input)
            .and(output.write_all(format!("Hello, World!\n```\n{}\n```\n", input).as_bytes()))
        {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        } else {
            std::process::exit(0);
        }
    }

    let args = Cli::parse();

    debug!("args: {:?}", args);

    config::ensure_config_files()
        .expect("Unable to verify that the config files exist or to generate new ones.");

    let is_piped = !stdin.is_terminal();
    let mut prompt_customizaton_text: Option<String> = None;

    let prompt = if args.extend_conversation {
        prompt_customizaton_text = args.input_or_template_ref.clone();

        if args.input_if_template_ref.is_some() {
            panic!(
                "Invalid parameters, cannot provide a config ref when extending a conversation.\n\
                Use `sc -e \"<your_prompt>.\"`"
            );
        }

        match get_last_conversation_as_prompt(args.name.as_deref()) {
            Some(prompt) => prompt,
            None => {
                if args.name.is_some() {
                    panic!("Named conversation does not exist: {}", args.name.unwrap());
                }
                get_default_and_or_custom_prompt(&args, &mut prompt_customizaton_text)
            }
        }
    } else {
        // try to get prompt matching the first arg and use second arg as customization text
        // if it doesn't use default prompt and treat that first arg as customization text
        get_default_and_or_custom_prompt(&args, &mut prompt_customizaton_text)
    };

    // if no text was piped, use the custom prompt as input
    if is_piped {
        stdin.lock().read_to_string(&mut input).unwrap();
    }

    if input.is_empty() {
        input.push_str(&prompt_customizaton_text.unwrap_or_default());
        prompt_customizaton_text = None;
    }

    debug!("input: {}", input);
    debug!("promt_customization_text: {:?}", prompt_customizaton_text);

    let prompt = customize_prompt(prompt, &args.prompt_params, prompt_customizaton_text);

    debug!("{:?}", prompt);

    match process_input_with_request(prompt, input, &mut output, args.repeat_input) {
        Ok(new_prompt) => {
        save_conversation(&new_prompt, args.name.as_deref())
            .expect("Failed to save conversation");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ensure_config_usable();
            std::process::exit(1);
        }
    }
}

/// Fills prompt_customization_text with the correct part of the args
/// first arg -> input_or_config_ref
/// second arg -> input_if_config_ref
/// if first arg is a prompt name, get that prompt and use second arg as input
/// if not, use default prompt, use first arg as input and forbid second arg
fn get_default_and_or_custom_prompt(
    args: &Cli,
    prompt_customization_text: &mut Option<String>,
) -> Prompt {
    let mut prompts = get_prompts();
    let available_prompts: Vec<&String> = prompts.keys().collect();
    let prompt_not_found_error = format!(
        "`default` prompt not found, available ones are: {:?}",
        &available_prompts
    );

    let input_or_config_ref = args
        .input_or_template_ref
        .clone()
        .unwrap_or_else(|| String::from("default"));

    if let Some(prompt) = prompts.remove(&input_or_config_ref) {
        if args.input_if_template_ref.is_some() {
            // first arg matching a prompt and second one is customization
            *prompt_customization_text = args.input_if_template_ref.clone()
        }
        prompt
    } else {
        *prompt_customization_text = Some(input_or_config_ref);
        if args.input_if_template_ref.is_some() {
            // first arg isn't a prompt and a second one was provided
            panic!(
                "Invalid parameters, either provide a valid ref to a config prompt then an input, or only an input.\n\
                Use `sc <config_ref> \"<your_prompt\"` or `sc \"<your_prompt>\"`"
            );
        }

        prompts
            .remove(DEFAULT_PROMPT_NAME)
            .expect(&prompt_not_found_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::prompt::{Prompt, Message};
    use tempfile::tempdir;
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
    fn test_cli_with_nonexistent_conversation() {
        let _temp_dir = setup();

        let args = Cli {
            input_or_template_ref: Some("test_input".to_string()),
            input_if_template_ref: None,
            extend_conversation: true,
            repeat_input: false,
            name: Some("nonexistent_conversation".to_string()),
            prompt_params: PromptParams::default(),
        };

        // Test that getting a nonexistent conversation returns None
        let prompt = get_last_conversation_as_prompt(args.name.as_deref());
        assert!(prompt.is_none());
    }

    #[test]
    #[serial]
    fn test_cli_with_existing_conversation() {
        let _temp_dir = setup();

        // Create a test conversation
        let test_prompt = create_test_prompt();
        save_conversation(&test_prompt, Some("test_conversation")).unwrap();

        let args = Cli {
            input_or_template_ref: Some("test_input".to_string()),
            input_if_template_ref: None,
            extend_conversation: true,
            repeat_input: false,
            name: Some("test_conversation".to_string()),
            prompt_params: PromptParams::default(),
        };

        // Test retrieving the saved conversation
        let prompt = get_last_conversation_as_prompt(args.name.as_deref());
        assert!(prompt.is_some());
        assert_eq!(prompt.unwrap(), test_prompt);
    }

    #[test]
    #[serial]
    fn test_valid_conversation_name() {
        assert!(valid_conversation_name("valid_name").is_ok());
        assert!(valid_conversation_name("valid-name").is_ok());
        assert!(valid_conversation_name("valid123").is_ok());
        assert!(valid_conversation_name("VALID_NAME").is_ok());

        assert!(valid_conversation_name("invalid name").is_err());
        assert!(valid_conversation_name("invalid/name").is_err());
        assert!(valid_conversation_name("invalid.name").is_err());
        assert!(valid_conversation_name("").is_err());
    }

    #[test]
    #[serial]
    fn test_conversation_persistence() {
        let _temp_dir = setup();
        let test_prompt = create_test_prompt();

        // Test saving and loading default conversation
        save_conversation(&test_prompt, None).unwrap();
        let loaded_prompt = get_last_conversation_as_prompt(None);
        assert!(loaded_prompt.is_some());
        assert_eq!(loaded_prompt.unwrap(), test_prompt);

        // Test saving and loading named conversation
        save_conversation(&test_prompt, Some("test_conv")).unwrap();
        let loaded_named_prompt = get_last_conversation_as_prompt(Some("test_conv"));
        assert!(loaded_named_prompt.is_some());
        assert_eq!(loaded_named_prompt.unwrap(), test_prompt);
    }

    #[test]
    #[serial]
    fn test_default_prompt_fallback() {
        let _temp_dir = setup();
        let args = Cli {
            input_or_template_ref: Some("test_input".to_string()),
            input_if_template_ref: None,
            extend_conversation: true,
            repeat_input: false,
            name: None,
            prompt_params: PromptParams::default(),
        };

        let prompt = get_last_conversation_as_prompt(args.name.as_deref());
        assert!(prompt.is_none()); // Should be None when no conversation exists

        // Verify the prompt customization text is set correctly
        let prompt_customization_text = args.input_or_template_ref;
        assert_eq!(prompt_customization_text, Some("test_input".to_string()));
    }

}
