use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCatalog {
    pub skills: Vec<SkillDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDocument {
    pub path: PathBuf,
    pub metadata: SkillFrontmatter,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default, alias = "allowed-tools")]
    pub allowed_tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSelectionRequest {
    pub explicit_skills: Vec<String>,
    pub user_input: String,
}

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("failed to read skill file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("missing frontmatter in skill file {path}")]
    MissingFrontmatter { path: PathBuf },
    #[error("invalid frontmatter in skill file {path}: {source}")]
    InvalidFrontmatter {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },
    #[error("explicit skill not found: {name}")]
    ExplicitSkillNotFound { name: String },
}

pub fn discover_skills(roots: &[PathBuf]) -> Result<SkillCatalog, SkillError> {
    let mut skills = Vec::new();

    for root in roots {
        if !root.exists() {
            continue;
        }

        for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() || entry.file_name() != "SKILL.md" {
                continue;
            }

            let path = entry.into_path();
            let content = fs::read_to_string(&path).map_err(|source| SkillError::ReadFile {
                path: path.clone(),
                source,
            })?;
            skills.push(parse_skill_document(&path, &content)?);
        }
    }

    skills.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(SkillCatalog { skills })
}

pub fn select_skills(
    request: &SkillSelectionRequest,
    skills: &[SkillDocument],
) -> Result<Vec<SkillDocument>, SkillError> {
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    let input = normalize_for_matching(&request.user_input);
    let input_tokens = tokenize(&request.user_input);

    for explicit_name in &request.explicit_skills {
        let skill = skills
            .iter()
            .find(|skill| skill.metadata.name.eq_ignore_ascii_case(explicit_name))
            .ok_or_else(|| SkillError::ExplicitSkillNotFound {
                name: explicit_name.clone(),
            })?;
        push_unique(&mut selected, &mut seen, skill);
    }

    for skill in skills {
        let trigger_matches = skill
            .metadata
            .trigger
            .as_ref()
            .map(|trigger| input.contains(&normalize_for_matching(trigger)))
            .unwrap_or(false);
        if trigger_matches {
            push_unique(&mut selected, &mut seen, skill);
        }
    }

    for skill in skills {
        let description_tokens = tokenize(&skill.metadata.description);
        if !description_tokens.is_empty()
            && description_tokens
                .iter()
                .any(|token| input_tokens.contains(token))
        {
            push_unique(&mut selected, &mut seen, skill);
        }
    }

    Ok(selected)
}

pub fn render_injected_skills(skills: &[SkillDocument]) -> String {
    skills
        .iter()
        .map(|skill| {
            format!(
                "Activated skill: {}\nPath: {}\n{}\n",
                skill.metadata.name,
                skill.path.display(),
                skill.body.trim()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_skill_document(path: &Path, content: &str) -> Result<SkillDocument, SkillError> {
    let normalized = content.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .ok_or_else(|| SkillError::MissingFrontmatter {
            path: path.to_path_buf(),
        })?;
    let (frontmatter, body) =
        rest.split_once("\n---\n")
            .ok_or_else(|| SkillError::MissingFrontmatter {
                path: path.to_path_buf(),
            })?;
    let metadata: SkillFrontmatter =
        serde_yaml::from_str(frontmatter).map_err(|source| SkillError::InvalidFrontmatter {
            path: path.to_path_buf(),
            source,
        })?;

    Ok(SkillDocument {
        path: path.to_path_buf(),
        metadata,
        body: body.trim().to_string(),
    })
}

fn push_unique(
    selected: &mut Vec<SkillDocument>,
    seen: &mut HashSet<String>,
    skill: &SkillDocument,
) {
    let key = skill.metadata.name.to_ascii_lowercase();
    if seen.insert(key) {
        selected.push(skill.clone());
    }
}

fn normalize_for_matching(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn tokenize(value: &str) -> HashSet<String> {
    value
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.len() >= 4)
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::{
        SkillDocument, SkillFrontmatter, SkillSelectionRequest, discover_skills,
        render_injected_skills, select_skills,
    };
    use tempfile::tempdir;

    #[test]
    fn discover_skills_reads_frontmatter_and_body() {
        let root = tempdir().unwrap();
        let skill_dir = root.path().join("skills").join("log-dashboard");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: log-dashboard
description: Turn logs into dashboards
trigger: dashboard
allowed-tools:
  - rg
  - fd
---
# Log Dashboard
Focus on top errors.
"#,
        )
        .unwrap();

        let catalog = discover_skills(&[root.path().join("skills")]).unwrap();

        assert_eq!(catalog.skills.len(), 1);
        let skill = &catalog.skills[0];
        assert_eq!(skill.metadata.name, "log-dashboard");
        assert_eq!(skill.metadata.description, "Turn logs into dashboards");
        assert_eq!(skill.metadata.trigger.as_deref(), Some("dashboard"));
        assert_eq!(skill.metadata.allowed_tools, vec!["rg", "fd"]);
        assert!(skill.body.contains("Focus on top errors."));
    }

    #[test]
    fn select_skills_prefers_explicit_then_trigger_then_description() {
        let skills = vec![
            sample_skill(
                "explicit-skill",
                "General helper",
                Some("ignore-me"),
                "# explicit",
            ),
            sample_skill(
                "trigger-skill",
                "Used for dashboards",
                Some("dashboard"),
                "# trigger",
            ),
            sample_skill(
                "description-skill",
                "Transform logs into tables",
                None,
                "# description",
            ),
        ];

        let selected = select_skills(
            &SkillSelectionRequest {
                explicit_skills: vec!["explicit-skill".to_string()],
                user_input: "please build a dashboard from these logs".to_string(),
            },
            &skills,
        )
        .unwrap();

        let names: Vec<_> = selected
            .iter()
            .map(|skill| skill.metadata.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["explicit-skill", "trigger-skill", "description-skill"]
        );
    }

    #[test]
    fn select_skills_fails_for_missing_explicit_skill() {
        let error = select_skills(
            &SkillSelectionRequest {
                explicit_skills: vec!["missing-skill".to_string()],
                user_input: "build dashboard".to_string(),
            },
            &[sample_skill("known", "Known helper", None, "# known")],
        )
        .unwrap_err();

        assert!(matches!(
            error,
            crate::SkillError::ExplicitSkillNotFound { ref name } if name == "missing-skill"
        ));
    }

    #[test]
    fn render_injected_skills_wraps_selected_bodies_for_prompting() {
        let selected = vec![
            sample_skill(
                "log-dashboard",
                "Dashboards from logs",
                Some("dashboard"),
                "# Dashboard\nUse charts.",
            ),
            sample_skill(
                "incident-summary",
                "Summaries from incidents",
                None,
                "# Incident\nUse timelines.",
            ),
        ];

        let injected = render_injected_skills(&selected);

        assert!(injected.contains("Activated skill: log-dashboard"));
        assert!(injected.contains("# Dashboard"));
        assert!(injected.contains("Activated skill: incident-summary"));
        assert!(injected.contains("# Incident"));
    }

    fn sample_skill(
        name: &str,
        description: &str,
        trigger: Option<&str>,
        body: &str,
    ) -> SkillDocument {
        SkillDocument {
            path: PathBuf::from(format!("/{name}/SKILL.md")),
            metadata: SkillFrontmatter {
                name: name.to_string(),
                description: description.to_string(),
                trigger: trigger.map(ToString::to_string),
                allowed_tools: Vec::new(),
            },
            body: body.to_string(),
        }
    }
}
