use crate::config::Config;
use crate::github::types::GhIssue;

/// Builds structured prompts for Claude sessions based on issue type and config.
pub struct PromptBuilder;

impl PromptBuilder {
    /// Build a structured prompt for an issue-based session.
    pub fn build_issue_prompt(issue: &GhIssue, config: &Config) -> String {
        let task_type = Self::detect_task_type(issue);
        let phase_instructions = Self::phase_instructions(&task_type);
        let safety_guards = Self::safety_guards();
        let reasoning_steps = Self::structured_reasoning();

        format!(
            "Work on GitHub issue #{issue_number}.\n\n\
             Title: {title}\n\n\
             Description:\n{body}\n\n\
             ## Task Type: {task_type_label}\n\n\
             {phase_instructions}\n\n\
             ## Safety Guards\n\n\
             {safety_guards}\n\n\
             ## Approach\n\n\
             {reasoning_steps}\n\n\
             ## Runtime Context\n\n\
             - Base branch: {base_branch}\n\
             - Model: {model}\n\
             - Mode: {mode}\n\n\
             IMPORTANT: You are running in unattended mode (no human at the terminal). \
             Do NOT use AskUserQuestion or ask for clarification — make your best judgment \
             and proceed autonomously.",
            issue_number = issue.number,
            title = issue.title,
            body = issue.body,
            task_type_label = task_type,
            phase_instructions = phase_instructions,
            safety_guards = safety_guards,
            reasoning_steps = reasoning_steps,
            base_branch = config.project.base_branch,
            model = config.sessions.default_model,
            mode = config.sessions.default_mode,
        )
    }

    /// Detect the task type from issue labels.
    fn detect_task_type(issue: &GhIssue) -> String {
        for label in &issue.labels {
            match label.as_str() {
                "type:docs" | "documentation" => return "Documentation".into(),
                "type:refactor" | "refactor" => return "Refactoring".into(),
                "type:bug" | "bug" => return "Bug Fix".into(),
                "type:test" | "test" => return "Testing".into(),
                "type:feature" | "enhancement" => return "Feature".into(),
                "type:backend" => return "Backend".into(),
                _ => {}
            }
        }
        "General".into()
    }

    /// Phase-specific instructions based on task type.
    fn phase_instructions(task_type: &str) -> String {
        match task_type {
            "Documentation" => {
                "Focus on documentation quality:\n\
                 - Read existing docs to understand the style and conventions\n\
                 - Update or create documentation files as needed\n\
                 - Ensure code examples are accurate and tested\n\
                 - Do NOT modify source code unless fixing doc comments"
                    .into()
            }
            "Refactoring" => {
                "Focus on preserving existing behavior:\n\
                 - Run tests BEFORE making any changes to establish a baseline\n\
                 - Make incremental changes, running tests after each step\n\
                 - Do NOT change public API signatures unless explicitly requested\n\
                 - Do NOT add new features — only restructure existing code"
                    .into()
            }
            "Bug Fix" => {
                "Follow a reproduce-first approach:\n\
                 - First, understand the bug by reading related code and tests\n\
                 - Write a failing test that reproduces the bug\n\
                 - Fix the bug with the minimum change necessary\n\
                 - Verify the fix by running all tests\n\
                 - Do NOT refactor surrounding code — fix only the bug"
                    .into()
            }
            "Testing" => {
                "Focus on test coverage:\n\
                 - Read the source code to understand the behavior to test\n\
                 - Write tests for edge cases and error conditions\n\
                 - Ensure tests are deterministic and do not depend on external state\n\
                 - Do NOT modify source code — only add or update tests"
                    .into()
            }
            _ => {
                "Follow standard development practices:\n\
                 - Read and understand the codebase before making changes\n\
                 - Write tests for new functionality\n\
                 - Keep changes focused on the issue scope"
                    .into()
            }
        }
    }

    /// Safety guards common to all task types.
    fn safety_guards() -> &'static str {
        "- Do NOT delete existing test files\n\
         - Run tests before committing changes\n\
         - Do NOT modify files outside the issue scope\n\
         - Do NOT introduce breaking changes to public APIs without explicit instruction\n\
         - Do NOT add unnecessary dependencies"
    }

    /// Structured reasoning steps.
    fn structured_reasoning() -> &'static str {
        "1. Read and understand the relevant codebase sections\n\
         2. Plan your approach — identify which files need changes\n\
         3. Implement changes incrementally\n\
         4. Run tests to verify your changes\n\
         5. Review your changes for correctness and completeness"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_issue(labels: &[&str]) -> GhIssue {
        GhIssue {
            number: 42,
            title: "Test issue".into(),
            body: "Fix the thing".into(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            state: "open".into(),
            html_url: String::new(),
        }
    }

    fn make_config() -> Config {
        toml::from_str(
            r#"
            [project]
            repo = "owner/repo"
            base_branch = "main"
            [sessions]
            [budget]
            [github]
            [notifications]
            "#,
        )
        .unwrap()
    }

    #[test]
    fn build_issue_prompt_contains_issue_number_and_title() {
        let issue = make_issue(&[]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("#42"));
        assert!(prompt.contains("Test issue"));
        assert!(prompt.contains("Fix the thing"));
    }

    #[test]
    fn build_issue_prompt_contains_safety_guards() {
        let issue = make_issue(&[]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("Do NOT delete existing test files"));
        assert!(prompt.contains("Run tests before committing"));
    }

    #[test]
    fn build_issue_prompt_contains_unattended_mode() {
        let issue = make_issue(&[]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("unattended mode"));
    }

    #[test]
    fn detect_task_type_docs() {
        let issue = make_issue(&["type:docs"]);
        assert_eq!(PromptBuilder::detect_task_type(&issue), "Documentation");
    }

    #[test]
    fn detect_task_type_bug() {
        let issue = make_issue(&["type:bug"]);
        assert_eq!(PromptBuilder::detect_task_type(&issue), "Bug Fix");
    }

    #[test]
    fn detect_task_type_refactor() {
        let issue = make_issue(&["type:refactor"]);
        assert_eq!(PromptBuilder::detect_task_type(&issue), "Refactoring");
    }

    #[test]
    fn detect_task_type_feature() {
        let issue = make_issue(&["type:feature"]);
        assert_eq!(PromptBuilder::detect_task_type(&issue), "Feature");
    }

    #[test]
    fn detect_task_type_default_general() {
        let issue = make_issue(&["priority:P0"]);
        assert_eq!(PromptBuilder::detect_task_type(&issue), "General");
    }

    #[test]
    fn bug_prompt_contains_reproduce_first() {
        let issue = make_issue(&["type:bug"]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("reproduce-first"));
    }

    #[test]
    fn refactor_prompt_contains_preserve_behavior() {
        let issue = make_issue(&["type:refactor"]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("preserving existing behavior"));
    }

    #[test]
    fn docs_prompt_contains_documentation_focus() {
        let issue = make_issue(&["type:docs"]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("documentation quality"));
    }

    #[test]
    fn prompt_contains_base_branch() {
        let issue = make_issue(&[]);
        let config = make_config();
        let prompt = PromptBuilder::build_issue_prompt(&issue, &config);
        assert!(prompt.contains("Base branch: main"));
    }
}
