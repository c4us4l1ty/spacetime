//! Cortex adapters and sanitized prompt envelopes.
//!
//! Supported adapter roles per BUILD-PLAN.md:
//! - `Scripted`: deterministic `.task` fixture adapter.
//! - `CliAgent`: offline harness/adapter specification for local CLI agents (`codex`, `claude`,
//!   `opencode`, `hermes`) over line-delimited JSON without storing API keys in the supervisor.
//! - `LocalModel`: local model runtime boundary (e.g. llama.cpp / Ollama local process) with
//!   explicit model digest and zero network assumptions.
//! - `WebChatTab`: isolated browser tab adapter whose outputs always carry `Taint::UntrustedWeb`.

use spacetime_types::surface::SemanticView;
use spacetime_types::{
    canonical_json_string, sha256_hex, Action, Result, SpacetimeError, Taint,
};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CortexAdapterKind {
    Scripted,
    CliAgent,
    LocalModel,
    WebChatTab,
}

impl CortexAdapterKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scripted => "scripted",
            Self::CliAgent => "cli_agent",
            Self::LocalModel => "local_model",
            Self::WebChatTab => "web_chat_tab",
        }
    }

    pub fn output_taint(&self) -> Taint {
        match self {
            Self::Scripted | Self::CliAgent | Self::LocalModel => Taint::UntrustedToolOutput,
            Self::WebChatTab => Taint::UntrustedWeb,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CortexPromptEnvelope {
    pub turn_index: u64,
    pub goal_id: String,
    pub goal_title: String,
    pub steering_notes: Vec<String>,
    pub memory_snippets: Vec<String>,
    pub semantic_views: Vec<SemanticView>,
    pub envelope_digest: String,
}

impl CortexPromptEnvelope {
    pub fn build(
        turn_index: u64,
        goal_id: impl Into<String>,
        goal_title: impl Into<String>,
        steering_notes: Vec<String>,
        memory_snippets: Vec<String>,
        raw_views: Vec<SemanticView>,
    ) -> Result<Self> {
        let goal_id = goal_id.into();
        let goal_title = goal_title.into();
        let semantic_views: Vec<SemanticView> = raw_views
            .into_iter()
            .map(|mut view| {
                // Enforce defense-in-depth masking before any Cortex adapter receives UI state.
                view.raw_screenshot_allowed_for_cortex = false;
                for field in &mut view.fields {
                    if field.is_secret
                        || field.field_type.eq_ignore_ascii_case("password")
                        || field.name.to_ascii_lowercase().contains("password")
                        || field.name.to_ascii_lowercase().contains("token")
                    {
                        field.is_secret = true;
                        field.value = "[MASKED]".to_string();
                        view.contains_secret_fields = true;
                    }
                }
                view
            })
            .collect();

        let digest_input = canonical_json_string(&(
            turn_index,
            &goal_id,
            &goal_title,
            &steering_notes,
            &memory_snippets,
            &semantic_views,
        ))?;
        let envelope_digest = sha256_hex(digest_input.as_bytes());
        Ok(Self {
            turn_index,
            goal_id,
            goal_title,
            steering_notes,
            memory_snippets,
            semantic_views,
            envelope_digest,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterConfig {
    pub kind: CortexAdapterKind,
    pub binary_or_profile: String,
    pub args: Vec<String>,
    pub max_context_tokens: usize,
    pub local_only: bool,
}

impl AdapterConfig {
    pub fn cli_agent(binary: impl Into<String>, args: Vec<String>) -> Result<Self> {
        let binary = binary.into();
        if binary.is_empty() || binary.contains(' ') {
            return Err(SpacetimeError::new(
                "cli agent binary must be a non-empty executable name",
            ));
        }
        Ok(Self {
            kind: CortexAdapterKind::CliAgent,
            binary_or_profile: binary,
            args,
            max_context_tokens: 8_192,
            local_only: false,
        })
    }

    pub fn local_model(profile: impl Into<String>) -> Self {
        Self {
            kind: CortexAdapterKind::LocalModel,
            binary_or_profile: profile.into(),
            args: vec!["--offline".to_string()],
            max_context_tokens: 8_192,
            local_only: true,
        }
    }

    pub fn web_chat_tab(profile: impl Into<String>) -> Self {
        Self {
            kind: CortexAdapterKind::WebChatTab,
            binary_or_profile: profile.into(),
            args: Vec::new(),
            max_context_tokens: 4_096,
            local_only: false,
        }
    }
}

pub trait TurnAdapter {
    fn kind(&self) -> CortexAdapterKind;
    fn propose_actions(&mut self, envelope: &CortexPromptEnvelope) -> Result<Vec<Action>>;
}

/// Deterministic offline adapter backed by a keyed map of envelope/turn index -> actions.
#[derive(Debug, Default)]
pub struct OfflineAdapterStub {
    pub config: Option<AdapterConfig>,
    pub kind: Option<CortexAdapterKind>,
    pub plans: BTreeMap<u64, Vec<Action>>,
}

impl OfflineAdapterStub {
    pub fn new(kind: CortexAdapterKind) -> Self {
        Self {
            config: None,
            kind: Some(kind),
            plans: BTreeMap::new(),
        }
    }

    pub fn insert_turn_plan(&mut self, turn_index: u64, actions: Vec<Action>) {
        self.plans.insert(turn_index, actions);
    }
}

impl TurnAdapter for OfflineAdapterStub {
    fn kind(&self) -> CortexAdapterKind {
        self.kind.unwrap_or(CortexAdapterKind::Scripted)
    }

    fn propose_actions(&mut self, envelope: &CortexPromptEnvelope) -> Result<Vec<Action>> {
        Ok(self
            .plans
            .get(&envelope.turn_index)
            .cloned()
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetime_types::surface::SemanticField;
    use spacetime_types::{SurfaceId, WorkspaceId};

    #[test]
    fn prompt_envelope_masks_secrets_and_blocks_raw_screenshots() -> Result<()> {
        let view = SemanticView {
            workspace_id: WorkspaceId::new("ws-upload"),
            surface_id: SurfaceId::new("surface-upload"),
            fields: vec![
                SemanticField {
                    name: "title".to_string(),
                    field_type: "text".to_string(),
                    value: "Demo".to_string(),
                    is_secret: false,
                },
                SemanticField {
                    name: "api_password".to_string(),
                    field_type: "text".to_string(),
                    value: "secret-should-not-leak".to_string(),
                    is_secret: false,
                },
            ],
            contains_secret_fields: false,
            raw_screenshot_allowed_for_cortex: true,
        };
        let env = CortexPromptEnvelope::build(
            1,
            "goal-1",
            "Upload draft",
            vec!["keep dry run".to_string()],
            vec!["fact: previous run passed".to_string()],
            vec![view],
        )?;
        assert!(!env.semantic_views[0].raw_screenshot_allowed_for_cortex);
        assert!(env.semantic_views[0].contains_secret_fields);
        assert_eq!(env.semantic_views[0].fields[1].value, "[MASKED]");
        assert_eq!(CortexAdapterKind::WebChatTab.output_taint(), Taint::UntrustedWeb);
        Ok(())
    }
}
