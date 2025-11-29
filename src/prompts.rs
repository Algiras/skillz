use anyhow::Result;
use rmcp::model::{GetPromptResult, ListPromptsResult, Prompt, PromptArgument, PromptMessage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A Skill/Prompt definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPrompt {
    pub name: String,
    pub description: String,
    pub arguments: Vec<PromptArgument>,
    pub messages: Vec<PromptMessage>,
}

impl SkillPrompt {
    pub fn new(name: String, description: String) -> Self {
        Self {
            name,
            description,
            arguments: Vec::new(),
            messages: Vec::new(),
        }
    }
}

/// Manages prompts/skills
#[derive(Clone)]
pub struct PromptRegistry {
    prompts: HashMap<String, SkillPrompt>,
}

impl PromptRegistry {
    pub fn new() -> Self {
        Self {
            prompts: HashMap::new(),
        }
    }

    pub fn register_prompt(&mut self, prompt: SkillPrompt) {
        self.prompts.insert(prompt.name.clone(), prompt);
    }

    pub fn get_prompt(&self, name: &str) -> Option<&SkillPrompt> {
        self.prompts.get(name)
    }

    pub fn list_prompts(&self) -> Vec<Prompt> {
        self.prompts
            .values()
            .map(|p| Prompt {
                name: p.name.clone(),
                description: Some(p.description.clone()),
                arguments: Some(p.arguments.clone()),
                title: None,
                icons: None,
            })
            .collect()
    }
}
