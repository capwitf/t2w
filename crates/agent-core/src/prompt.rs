use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectedSkill {
    pub name: String,
    pub instructions: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSnapshot {
    pub instruction: String,
    pub stdin: String,
    pub working_directory: PathBuf,
    pub workspace_files: Vec<String>,
    pub injected_skills: Vec<InjectedSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
}

pub fn build_prompt_request(context: &ContextSnapshot, model: String) -> PromptRequest {
    let skill_block = context
        .injected_skills
        .iter()
        .map(|skill| format!("Activated skill: {}\n{}", skill.name, skill.instructions))
        .collect::<Vec<_>>()
        .join("\n\n");
    let files = if context.workspace_files.is_empty() {
        "(none)".to_string()
    } else {
        context.workspace_files.join("\n")
    };

    PromptRequest {
        model,
        system_prompt: format!(
            "You are t2w, a terminal-to-web artifact engine.\n\
             Return only HTML. Do not wrap the response in Markdown fences.\n\
             Produce a self-contained document with inline CSS and JavaScript.\n\
             Keep the document safe for sandboxed iframe rendering.\n\n\
             {skill_block}"
        ),
        user_prompt: format!(
            "Instruction:\n{}\n\n\
             Working directory:\n{}\n\n\
             Workspace files:\n{}\n\n\
             STDIN:\n{}",
            context.instruction,
            context.working_directory.display(),
            files,
            context.stdin
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::prompt::{ContextSnapshot, InjectedSkill, build_prompt_request};

    #[test]
    fn build_prompt_includes_html_contract_skills_and_workspace_files() {
        let request = build_prompt_request(
            &ContextSnapshot {
                instruction: "Build an error dashboard".to_string(),
                stdin: "500 /health".to_string(),
                working_directory: PathBuf::from("C:/work/t2w"),
                workspace_files: vec!["Cargo.toml".to_string(), "logs/app.log".to_string()],
                injected_skills: vec![InjectedSkill {
                    name: "log-dashboard".to_string(),
                    instructions: "# Dashboard\nUse charts.".to_string(),
                }],
            },
            "claude-sonnet-4-5".to_string(),
        );

        assert_eq!(request.model, "claude-sonnet-4-5");
        assert!(request.system_prompt.contains("Return only HTML"));
        assert!(
            request
                .system_prompt
                .contains("Activated skill: log-dashboard")
        );
        assert!(request.user_prompt.contains("Build an error dashboard"));
        assert!(request.user_prompt.contains("500 /health"));
        assert!(request.user_prompt.contains("Cargo.toml"));
    }
}
