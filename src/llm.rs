use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{
    game::{Contract, Difficulty, Location},
    settings::{LlmSettings, SecretStore},
};

pub(crate) struct GeneratedContractFlavor {
    pub(crate) title: String,
    pub(crate) briefing: String,
}

pub(crate) trait ContractFlavorGenerator {
    fn generate_flavor(
        &self,
        settings: &LlmSettings,
        secret_store: &dyn SecretStore,
        contract: &Contract,
        difficulty: Difficulty,
        locations: &[Location],
    ) -> Result<GeneratedContractFlavor, String>;
}

pub(crate) struct OpenAiCompatibleContractFlavorGenerator;

impl ContractFlavorGenerator for OpenAiCompatibleContractFlavorGenerator {
    fn generate_flavor(
        &self,
        settings: &LlmSettings,
        secret_store: &dyn SecretStore,
        contract: &Contract,
        difficulty: Difficulty,
        locations: &[Location],
    ) -> Result<GeneratedContractFlavor, String> {
        let api_key = secret_store
            .get_api_key()?
            .ok_or_else(|| "no API key configured".to_string())?;

        let origin = locations
            .get(contract.origin)
            .map(|location| location.name)
            .unwrap_or("Unknown");
        let destination = locations
            .get(contract.destination)
            .map(|location| location.name)
            .unwrap_or("Unknown");

        let client = Client::builder()
            .timeout(Duration::from_secs(settings.timeout_secs.max(5)))
            .build()
            .map_err(|error| error.to_string())?;

        let body = json!({
            "model": settings.model,
            "temperature": 0.8,
            "messages": [
                {
                    "role": "system",
                    "content": "You write compact JSON for a science-fiction courier game. Return only valid JSON with keys title and briefing. Keep title under 40 characters. Keep briefing under 180 characters. Do not include markdown or code fences."
                },
                {
                    "role": "user",
                    "content": format!(
                        "Create contract flavor text. Archetype: {}. Origin: {}. Destination: {}. Reward: {} credits. Target ETA: {} ticks. Difficulty: {}. Archetype effect: {}.",
                        contract.archetype.title(),
                        origin,
                        destination,
                        contract.reward,
                        contract.max_eta,
                        difficulty.label(),
                        contract.archetype.effect_summary(),
                    )
                }
            ]
        });

        let response = client
            .post(&settings.endpoint_url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .map_err(|error| error.to_string())?;

        let response = response
            .error_for_status()
            .map_err(|error| error.to_string())?;
        let payload: OpenAiCompatibleResponse =
            response.json().map_err(|error| error.to_string())?;
        let content = payload
            .choices
            .first()
            .map(|choice| choice.message.content.trim())
            .filter(|content| !content.is_empty())
            .ok_or_else(|| "empty LLM response".to_string())?;

        let parsed: FlavorPayload =
            serde_json::from_str(strip_code_fences(content)).map_err(|error| error.to_string())?;

        Ok(GeneratedContractFlavor {
            title: truncate(&parsed.title, 40),
            briefing: truncate(&parsed.briefing, 180),
        })
    }
}

#[derive(Deserialize)]
struct OpenAiCompatibleResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct FlavorPayload {
    title: String,
    briefing: String,
}

fn strip_code_fences(content: &str) -> &str {
    content
        .strip_prefix("```")
        .and_then(|stripped| stripped.strip_suffix("```"))
        .unwrap_or(content)
        .trim()
        .trim_start_matches("json")
        .trim()
}

fn truncate(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}
