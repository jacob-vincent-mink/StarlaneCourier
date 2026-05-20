use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{
    game::{Contract, Difficulty, Location, WorldFlavor, WorldLocationFlavor, WorldShipFlavor},
    settings::LlmSettings,
};

pub(crate) struct GeneratedContractFlavor {
    pub(crate) title: String,
    pub(crate) briefing: String,
}

#[derive(Clone, Copy)]
pub(crate) struct PromptSpec {
    pub(crate) name: &'static str,
    pub(crate) output: &'static str,
    pub(crate) system_prompt: &'static str,
    pub(crate) user_prompt_template: &'static str,
}

pub(crate) const CONTRACT_FLAVOR_PROMPT: PromptSpec = PromptSpec {
    name: "Contract Flavor",
    output: "JSON object with title and briefing",
    system_prompt: "You write compact JSON for a science-fiction courier game. Return only valid JSON with keys title and briefing. Keep title under 40 characters. Keep briefing under 180 characters. Do not include markdown or code fences.",
    user_prompt_template: "Create contract flavor text. Archetype: {archetype}. Origin: {origin}. Destination: {destination}. Reward: {reward} credits. Target ETA: {target_eta} ticks. Difficulty: {difficulty}. Archetype effect: {effect_summary}.",
};

pub(crate) const SECTOR_BOOTSTRAP_PROMPT: PromptSpec = PromptSpec {
    name: "Sector Bootstrap",
    output: "JSON object with environment name/summary, a variable number of role-ordered locations, three starter ships, and at least four shipyard offers",
    system_prompt: "You write compact JSON for a science-fiction courier game. Return only valid JSON. Create one environment package with keys environment_name, environment_summary, locations, starter_ships, and shipyard_offers. locations must be an array whose length is a multiple of 5, with at least 10 and at most 20 objects. Each consecutive group of 5 locations defines one sector and must preserve this role order: hub capital, frontier relay, industrial anchorage, rough harbor, remote signal relay. starter_ships must be an array of exactly 3 objects. shipyard_offers must be an array with at least 4 objects. Each location object must have keys region_name, sector_name, name, short_label, lane_name, description, cluster_name, and system_name. Each ship object must have keys name, class_name, and description. Keep environment_name under 40 characters. Keep environment_summary under 180 characters. Keep location name under 28 characters. Keep short_label under 10 characters. Keep lane_name under 24 characters. Keep description under 160 characters. Keep region_name, sector_name, cluster_name, and system_name under 24 characters. Keep ship name under 24 characters. Keep ship class_name under 24 characters. Keep ship description under 140 characters. Do not include markdown or code fences.",
    user_prompt_template: "Seed: {seed}. Create a frontier courier environment with 2 to 4 sectors spread across 1 to 3 regions. For every sector, preserve the five location roles in order: hub capital, frontier relay, industrial anchorage, rough harbor, remote signal relay. Make the sectors and regions feel distinct but connected through frontier logistics. Also create three starter ships and at least four shipyard featured hulls that fit the setting.",
};

pub(crate) const PROMPT_CATALOG: [PromptSpec; 2] =
    [CONTRACT_FLAVOR_PROMPT, SECTOR_BOOTSTRAP_PROMPT];

pub(crate) trait ContractFlavorGenerator: Send + Sync {
    fn test_connection(&self, settings: &LlmSettings, api_key: Option<&str>) -> Result<(), String>;

    fn generate_sector(
        &self,
        settings: &LlmSettings,
        api_key: Option<&str>,
        world_seed: u64,
    ) -> Result<WorldFlavor, String>;

    fn generate_flavor(
        &self,
        settings: &LlmSettings,
        api_key: Option<&str>,
        contract: &Contract,
        difficulty: Difficulty,
        locations: &[Location],
    ) -> Result<GeneratedContractFlavor, String>;
}

pub(crate) struct OpenAiCompatibleContractFlavorGenerator;

impl ContractFlavorGenerator for OpenAiCompatibleContractFlavorGenerator {
    fn test_connection(&self, settings: &LlmSettings, api_key: Option<&str>) -> Result<(), String> {
        let api_key = resolved_api_key(settings, api_key)?;

        let client = Client::builder()
            .timeout(Duration::from_secs(settings.timeout_secs.max(5)))
            .build()
            .map_err(|error| error.to_string())?;

        let mut request = client.get(normalize_models_endpoint(&settings.endpoint_url));
        if let Some(api_key) = api_key {
            request = request.bearer_auth(api_key);
        }

        request
            .send()
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;

        Ok(())
    }

    fn generate_flavor(
        &self,
        settings: &LlmSettings,
        api_key: Option<&str>,
        contract: &Contract,
        difficulty: Difficulty,
        locations: &[Location],
    ) -> Result<GeneratedContractFlavor, String> {
        let api_key = resolved_api_key(settings, api_key)?;

        let origin = locations
            .get(contract.origin)
            .map(|location| location.name.as_str())
            .unwrap_or("Unknown");
        let destination = locations
            .get(contract.destination)
            .map(|location| location.name.as_str())
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
                    "content": CONTRACT_FLAVOR_PROMPT.system_prompt
                },
                {
                    "role": "user",
                    "content": build_contract_flavor_prompt(contract, difficulty, origin, destination)
                }
            ]
        });

        let mut request = client.post(normalize_endpoint(&settings.endpoint_url));
        if let Some(api_key) = api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request
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

    fn generate_sector(
        &self,
        settings: &LlmSettings,
        api_key: Option<&str>,
        world_seed: u64,
    ) -> Result<WorldFlavor, String> {
        let api_key = resolved_api_key(settings, api_key)?;

        let client = Client::builder()
            .timeout(Duration::from_secs(settings.timeout_secs.max(5)))
            .build()
            .map_err(|error| error.to_string())?;

        let body = json!({
            "model": settings.model,
            "temperature": 0.9,
            "messages": [
                {
                    "role": "system",
                    "content": SECTOR_BOOTSTRAP_PROMPT.system_prompt
                },
                {
                    "role": "user",
                    "content": build_sector_prompt(world_seed)
                }
            ]
        });

        let mut request = client.post(normalize_endpoint(&settings.endpoint_url));
        if let Some(api_key) = api_key {
            request = request.bearer_auth(api_key);
        }

        let response = request
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

        let parsed: SectorPayload =
            serde_json::from_str(strip_code_fences(content)).map_err(|error| error.to_string())?;

        if parsed.locations.len() < 10
            || parsed.locations.len() > 20
            || parsed.locations.len() % 5 != 0
        {
            return Err(format!(
                "expected 10-20 generated locations in groups of 5, received {}",
                parsed.locations.len()
            ));
        }
        if parsed.starter_ships.len() != 3 {
            return Err(format!(
                "expected 3 starter ships, received {}",
                parsed.starter_ships.len()
            ));
        }
        if parsed.shipyard_offers.len() < 4 {
            return Err(format!(
                "expected at least 4 shipyard offers, received {}",
                parsed.shipyard_offers.len()
            ));
        }

        Ok(WorldFlavor {
            environment_name: truncate(&parsed.environment_name, 40),
            environment_summary: truncate(&parsed.environment_summary, 180),
            locations: parsed
                .locations
                .into_iter()
                .map(|location| WorldLocationFlavor {
                    region_name: truncate(&location.region_name, 24),
                    sector_name: truncate(&location.sector_name, 24),
                    name: truncate(&location.name, 28),
                    short_label: truncate(&location.short_label, 10),
                    lane_name: truncate(&location.lane_name, 24),
                    description: truncate(&location.description, 160),
                    cluster_name: truncate(&location.cluster_name, 24),
                    system_name: truncate(&location.system_name, 24),
                })
                .collect(),
            starter_ships: parsed
                .starter_ships
                .into_iter()
                .map(|ship| WorldShipFlavor {
                    name: truncate(&ship.name, 24),
                    class_name: truncate(&ship.class_name, 24),
                    description: truncate(&ship.description, 140),
                })
                .collect(),
            shipyard_offers: parsed
                .shipyard_offers
                .into_iter()
                .map(|ship| WorldShipFlavor {
                    name: truncate(&ship.name, 24),
                    class_name: truncate(&ship.class_name, 24),
                    description: truncate(&ship.description, 140),
                })
                .collect(),
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

#[derive(Deserialize)]
struct SectorPayload {
    environment_name: String,
    environment_summary: String,
    locations: Vec<SectorLocationPayload>,
    starter_ships: Vec<ShipFlavorPayload>,
    shipyard_offers: Vec<ShipFlavorPayload>,
}

#[derive(Deserialize)]
struct SectorLocationPayload {
    region_name: String,
    sector_name: String,
    name: String,
    short_label: String,
    lane_name: String,
    description: String,
    cluster_name: String,
    system_name: String,
}

#[derive(Deserialize)]
struct ShipFlavorPayload {
    name: String,
    class_name: String,
    description: String,
}

fn resolved_api_key<'a>(
    settings: &LlmSettings,
    api_key: Option<&'a str>,
) -> Result<Option<&'a str>, String> {
    if settings.provider.requires_api_key() {
        api_key
            .ok_or_else(|| "no API key configured".to_string())
            .map(Some)
    } else {
        Ok(None)
    }
}

fn build_contract_flavor_prompt(
    contract: &Contract,
    difficulty: Difficulty,
    origin: &str,
    destination: &str,
) -> String {
    CONTRACT_FLAVOR_PROMPT
        .user_prompt_template
        .replace("{archetype}", contract.archetype.title())
        .replace("{origin}", origin)
        .replace("{destination}", destination)
        .replace("{reward}", &contract.reward.to_string())
        .replace("{target_eta}", &contract.max_eta.to_string())
        .replace("{difficulty}", difficulty.label())
        .replace("{effect_summary}", contract.archetype.effect_summary())
}

fn build_sector_prompt(world_seed: u64) -> String {
    SECTOR_BOOTSTRAP_PROMPT
        .user_prompt_template
        .replace("{seed}", &world_seed.to_string())
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

fn normalize_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

fn normalize_models_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if let Some(prefix) = trimmed.strip_suffix("/chat/completions") {
        format!("{prefix}/models")
    } else if trimmed.ends_with("/models") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/models")
    }
}
