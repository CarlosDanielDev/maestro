pub fn build_analysis_prompt(profile_json: &str) -> String {
    format!(
        r#"You are analyzing a software project to produce an adaptation report.

## Project Profile

{profile_json}

## Instructions

Based on the project profile above, analyze the codebase and produce a JSON report with this exact schema:

```json
{{
  "summary": "A 2-3 sentence summary of the project's architecture and purpose",
  "modules": [
    {{
      "path": "src/module_name",
      "purpose": "What this module does",
      "complexity": "low|medium|high"
    }}
  ],
  "tech_debt_items": [
    {{
      "title": "Short title of the issue",
      "description": "Detailed description",
      "location": "src/file.rs:42 or src/module/",
      "suggested_fix": "How to fix this",
      "category": "dead_code|missing_tests|inconsistent_patterns|poor_abstractions|security_concern|documentation",
      "severity": "critical|high|medium|low"
    }}
  ]
}}
```

## Analysis Guidelines

1. **Modules**: Identify the main modules/packages and their purposes. Focus on the top-level organization.
2. **Tech Debt**: Look for:
   - Dead code (unused functions, modules, imports)
   - Missing tests (modules with no test coverage)
   - Inconsistent patterns (different approaches to the same problem)
   - Poor abstractions (god objects, tight coupling)
   - Security concerns (hardcoded secrets, unsafe patterns)
   - Missing documentation (public APIs without docs)

3. Be specific about locations — reference actual file paths from the project profile.
4. Be conservative with severity ratings — only use "critical" for genuine security or data-loss risks.

Return ONLY the JSON object, no markdown fences, no commentary."#,
    )
}

pub fn build_prd_prompt(profile_json: &str, report_json: &str) -> String {
    format!(
        r#"You are generating a Product Requirements Document (PRD) for a software project.

## Project Profile

{profile_json}

## Analysis Report

{report_json}

## Instructions

Generate a comprehensive PRD in markdown format. The document must contain ALL of the following sections:

# PRD: {{project name}}

## 1. Project Identity
- Name, description, primary language, tech stack summary

## 2. Architecture Overview
- Module map with responsibilities and boundaries
- How modules relate to each other

## 3. Component Inventory
For each key component:
- Purpose
- Complexity (low/medium/high)
- Dependencies on other components

## 4. Data Flow
- How data moves through the system
- Key entry points and exit points

## 5. Tech Stack
- Languages and versions
- Frameworks and libraries
- Build tools
- CI/CD pipeline
- Test frameworks

## 6. Current State
- Test coverage status
- Tech debt summary (reference the analysis report)
- Security posture

## 7. Non-Goals
- What this project intentionally does NOT do
- Explicit boundaries and out-of-scope items

Return ONLY the markdown document, no code fences wrapping the entire output."#,
    )
}

/// Build a prompt that asks Claude to ENRICH an existing PRD with the latest
/// analysis instead of regenerating from scratch. Used when PRD source
/// resolution finds an upstream document (local file, GitHub issue, or
/// Azure wiki page).
pub fn build_prd_enrich_prompt(
    profile_json: &str,
    report_json: &str,
    existing_prd: &str,
) -> String {
    format!(
        r#"You are updating a Product Requirements Document (PRD) for a software project.

## Existing PRD

{existing_prd}

## Project Profile (latest analysis)

{profile_json}

## Analysis Report (latest analysis)

{report_json}

## Instructions

Enrich the existing PRD with information from the latest analysis. Preserve
the voice, structure, and section headings that already exist. You MAY:

- Fill in missing sections (if the existing PRD omits any of the canonical
  sections: Project Identity, Architecture, Components, Data Flow, Tech
  Stack, Current State, Non-Goals).
- Correct facts that conflict with the latest analysis (new files, new
  frameworks, updated test coverage, resolved tech-debt items).
- Add bullets to existing sections when the analysis introduces new facts.
- Remove statements that the latest analysis has clearly invalidated.

You MUST NOT:

- Rewrite the entire document from scratch.
- Discard prose that the user appears to have hand-authored (non-generic
  paragraphs, rationale, trade-off notes, roadmap language).
- Change section headings that already exist, except to fix clear typos.

Return ONLY the updated markdown document, no code fences wrapping the
entire output, no commentary, no diff markers."#,
    )
}

pub fn build_planning_prompt(
    profile_json: &str,
    report_json: &str,
    milestone_naming_hint: Option<&str>,
    prd_content: Option<&str>,
) -> String {
    let naming_section = match milestone_naming_hint {
        Some(hint) => format!("\n6. **Milestone naming**: {}\n", hint),
        None => String::new(),
    };
    let prd_section = match prd_content {
        Some(prd) => format!("\n## Product Requirements Document\n\n{}\n", prd),
        None => String::new(),
    };
    format!(
        r#"You are creating a project adaptation plan to onboard a project to the maestro workflow.

## Project Profile

{profile_json}

## Analysis Report

{report_json}
{prd_section}

## Instructions

Create a structured plan with milestones and DOR-compliant issues. Return a JSON object with this exact schema:

```json
{{
  "milestones": [
    {{
      "title": "<short milestone title>",
      "description": "Description of the milestone goals",
      "issues": [
        {{
          "title": "feat: descriptive title",
          "body": "(DOR-compliant issue body with Overview, Expected Behavior, Acceptance Criteria, Files to Modify, Test Hints, Blocked By, Definition of Done sections)",
          "labels": ["enhancement"],
          "blocked_by_titles": []
        }}
      ]
    }}
  ],
  "maestro_toml_patch": "(optional suggested maestro.toml content)"
}}
```

## Planning Guidelines

1. **Milestones**: Group work into logical phases (e.g., Foundation, Testing, CI/CD, Integration)
2. **Issues**: Each issue should be:
   - Small enough to complete in one session (1-2 hours of AI work)
   - Self-contained with clear acceptance criteria
   - Properly ordered with `blocked_by_titles` referencing other issue titles
3. **Labels**: Use `enhancement`, `testing`, `documentation`, `tech-debt` as appropriate
4. **maestro_toml_patch**: Suggest initial configuration based on the project analysis
5. Prefix titles with `feat:`, `test:`, `chore:`, `fix:`, or `docs:` as appropriate
{naming_section}
Return ONLY the JSON object, no markdown fences, no commentary."#,
    )
}

pub fn build_scaffold_prompt(profile_json: &str, report_json: &str, plan_json: &str) -> String {
    format!(
        r#"You are generating a .claude/ directory scaffold for a software project to onboard it to the maestro AI-orchestrated workflow.

## Project Profile

{profile_json}

## Analysis Report

{report_json}

## Adaptation Plan

{plan_json}

## Instructions

Generate a JSON object containing all files to create in the `.claude/` directory. Each file has a relative path (from `.claude/`) and full content.

The scaffold MUST include:

### 1. CLAUDE.md (orchestrator configuration)
- Tailored to the detected tech stack
- Include: build commands, test commands, project structure, key files
- Include: TDD workflow, subagent delegation rules
- Stack-specific commands (e.g., `cargo test` for Rust, `npm test` for TypeScript, `pytest` for Python)

### 2. Commands (.claude/commands/)
- `implement.md` — fetch issue and implement with full workflow
- `pushup.md` — commit, push, create PR, close issue
- `release.md` — semantic version release workflow
- `simplify.md` — refactoring and simplification workflow

### 3. Subagents (.claude/agents/)
- `subagent-architect.md` — architecture design and planning (consultive only)
- `subagent-qa.md` — QA engineering, test design (consultive only)
- `subagent-docs-analyst.md` — documentation management (CAN write .md files)
- `subagent-security-analyst.md` — security review, OWASP (consultive only)

### 4. Skills (.claude/skills/)
- `project-patterns/SKILL.md` — project-specific patterns based on detected stack

## Stack-Specific Customization

Customize ALL content based on the detected language and stack:
- **Rust**: cargo build/test/clippy/fmt, mod.rs structure, async_trait patterns
- **TypeScript**: npm/yarn/pnpm, jest/vitest, ESLint, tsconfig
- **Python**: pytest, mypy, ruff/black, pyproject.toml, venv
- **Go**: go test, go vet, go fmt, go mod
- **Java**: gradle/maven, JUnit, checkstyle
- **Ruby**: rspec, rubocop, bundler

## Output Schema

```json
{{{{
  "files": [
    {{{{
      "path": "CLAUDE.md",
      "content": "(full CLAUDE.md content here)"
    }}}},
    {{{{
      "path": "commands/implement.md",
      "content": "(full implement.md content here)"
    }}}},
    {{{{
      "path": "agents/subagent-architect.md",
      "content": "(full subagent-architect.md content here)"
    }}}},
    {{{{
      "path": "skills/project-patterns/SKILL.md",
      "content": "(full SKILL.md content here)"
    }}}}
  ]
}}}}
```

Return ONLY the JSON object, no markdown fences, no commentary."#,
    )
}

/// Run `claude --print` with the given prompt and return stdout.
pub async fn run_claude_print(
    model: &str,
    prompt: &str,
    cwd: &std::path::Path,
) -> anyhow::Result<String> {
    let output = tokio::process::Command::new("claude")
        .args([
            "--print",
            "--output-format",
            "text",
            "--model",
            model,
            "-p",
            prompt,
        ])
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to spawn claude CLI: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Claude CLI failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Extract and parse a JSON block from Claude's response.
pub fn parse_json_response<T: serde::de::DeserializeOwned>(raw: &str) -> anyhow::Result<T> {
    let trimmed = raw.trim();

    // Try direct parse first
    if let Ok(v) = serde_json::from_str(trimmed) {
        return Ok(v);
    }

    // Try extracting from markdown code block
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(v) = serde_json::from_str(json_str) {
                return Ok(v);
            }
        }
    }

    // Try extracting from generic code block
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        // Skip the language tag if present
        let after = if let Some(nl) = after.find('\n') {
            &after[nl + 1..]
        } else {
            after
        };
        if let Some(end) = after.find("```") {
            let json_str = after[..end].trim();
            if let Ok(v) = serde_json::from_str(json_str) {
                return Ok(v);
            }
        }
    }

    // Try finding the first { to last }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_str = &trimmed[start..=end];
        if let Ok(v) = serde_json::from_str(json_str) {
            return Ok(v);
        }
    }

    anyhow::bail!(
        "Failed to parse JSON from Claude response. Raw response: {}",
        &trimmed[..trimmed.len().min(200)]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::types::{AdaptReport, ProjectProfile};

    #[test]
    fn build_analysis_prompt_contains_profile() {
        let prompt = build_analysis_prompt(r#"{"name":"test"}"#);
        assert!(prompt.contains(r#"{"name":"test"}"#));
        assert!(prompt.contains("tech_debt_items"));
        assert!(prompt.contains("modules"));
    }

    #[test]
    fn build_planning_prompt_contains_both_inputs() {
        let prompt =
            build_planning_prompt(r#"{"name":"test"}"#, r#"{"summary":"good"}"#, None, None);
        assert!(prompt.contains(r#"{"name":"test"}"#));
        assert!(prompt.contains(r#"{"summary":"good"}"#));
        assert!(prompt.contains("milestones"));
        assert!(prompt.contains("maestro_toml_patch"));
    }

    #[test]
    fn build_planning_prompt_includes_naming_hint_when_provided() {
        let prompt = build_planning_prompt(
            r#"{"name":"test"}"#,
            r#"{"summary":"good"}"#,
            Some("Use semver format: vX.Y.Z"),
            None,
        );
        assert!(prompt.contains("Use semver format: vX.Y.Z"));
    }

    #[test]
    fn build_planning_prompt_omits_naming_section_when_none() {
        let prompt =
            build_planning_prompt(r#"{"name":"test"}"#, r#"{"summary":"good"}"#, None, None);
        assert!(!prompt.contains("Milestone naming"));
    }

    #[test]
    fn parse_json_response_direct_json() {
        let raw = r#"{"summary":"test","modules":[],"tech_debt_items":[]}"#;
        let report: AdaptReport = parse_json_response(raw).unwrap();
        assert_eq!(report.summary, "test");
    }

    #[test]
    fn parse_json_response_markdown_fenced() {
        let raw = r#"Here is the result:

```json
{"summary":"fenced","modules":[],"tech_debt_items":[]}
```

Done!"#;
        let report: AdaptReport = parse_json_response(raw).unwrap();
        assert_eq!(report.summary, "fenced");
    }

    #[test]
    fn parse_json_response_generic_fence() {
        let raw = r#"```
{"summary":"generic","modules":[],"tech_debt_items":[]}
```"#;
        let report: AdaptReport = parse_json_response(raw).unwrap();
        assert_eq!(report.summary, "generic");
    }

    #[test]
    fn parse_json_response_with_surrounding_text() {
        let raw = r#"Here is the analysis:
{"summary":"embedded","modules":[],"tech_debt_items":[]}
That's all."#;
        let report: AdaptReport = parse_json_response(raw).unwrap();
        assert_eq!(report.summary, "embedded");
    }

    #[test]
    fn parse_json_response_invalid_returns_error() {
        let raw = "This is not JSON at all";
        let result: anyhow::Result<AdaptReport> = parse_json_response(raw);
        assert!(result.is_err());
    }

    #[test]
    fn build_prd_prompt_contains_profile_and_report() {
        let prompt = build_prd_prompt(r#"{"name":"test"}"#, r#"{"summary":"good"}"#);
        assert!(prompt.contains(r#"{"name":"test"}"#));
        assert!(prompt.contains(r#"{"summary":"good"}"#));
    }

    #[test]
    fn build_prd_prompt_contains_all_section_headings() {
        let prompt = build_prd_prompt(r#"{}"#, r#"{}"#);
        assert!(prompt.contains("Project Identity"));
        assert!(prompt.contains("Architecture Overview"));
        assert!(prompt.contains("Component Inventory"));
        assert!(prompt.contains("Data Flow"));
        assert!(prompt.contains("Tech Stack"));
        assert!(prompt.contains("Current State"));
        assert!(prompt.contains("Non-Goals"));
    }

    #[test]
    fn build_planning_prompt_includes_prd_when_provided() {
        let prompt = build_planning_prompt(
            r#"{"name":"test"}"#,
            r#"{"summary":"good"}"#,
            None,
            Some("# PRD: Test Project\n\nSome PRD content here"),
        );
        assert!(prompt.contains("Product Requirements Document"));
        assert!(prompt.contains("# PRD: Test Project"));
    }

    #[test]
    fn build_planning_prompt_prd_and_naming_hint_coexist() {
        let prompt = build_planning_prompt(
            r#"{"name":"test"}"#,
            r#"{"summary":"good"}"#,
            Some("Use semver format: vX.Y.Z"),
            Some("## PRD content here"),
        );
        assert!(prompt.contains("Use semver format: vX.Y.Z"));
        assert!(prompt.contains("## PRD content here"));
    }

    #[test]
    fn build_planning_prompt_omits_prd_when_none() {
        let prompt =
            build_planning_prompt(r#"{"name":"test"}"#, r#"{"summary":"good"}"#, None, None);
        assert!(!prompt.contains("Product Requirements Document"));
    }

    #[test]
    fn build_scaffold_prompt_contains_profile_report_and_plan() {
        let prompt = build_scaffold_prompt(
            r#"{"name":"test"}"#,
            r#"{"summary":"good"}"#,
            r#"{"milestones":[]}"#,
        );
        assert!(prompt.contains(r#"{"name":"test"}"#));
        assert!(prompt.contains(r#"{"summary":"good"}"#));
        assert!(prompt.contains(r#"{"milestones":[]}"#));
    }

    #[test]
    fn build_scaffold_prompt_contains_required_sections() {
        let prompt = build_scaffold_prompt(r#"{}"#, r#"{}"#, r#"{}"#);
        assert!(prompt.contains("CLAUDE.md"));
        assert!(prompt.contains("commands/"));
        assert!(prompt.contains("agents/"));
        assert!(prompt.contains("skills/"));
    }

    #[test]
    fn parse_json_response_profile() {
        let raw = r#"{"name":"p","root":"/tmp","language":"rust","manifests":[],"config_files":[],"entry_points":[],"source_stats":{"total_files":0,"total_lines":0,"by_extension":[]},"test_infra":{"has_tests":false,"framework":null,"test_directories":[],"test_file_count":0},"ci":{"provider":null,"config_files":[]},"git":{"is_git_repo":false,"default_branch":null,"remote_url":null,"commit_count":0,"recent_contributors":[]},"dependencies":{"direct_count":0,"dev_count":0,"notable":[]},"directory_tree":"","has_maestro_config":false}"#;
        let profile: ProjectProfile = parse_json_response(raw).unwrap();
        assert_eq!(profile.name, "p");
    }
}
