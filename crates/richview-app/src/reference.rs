//! Public reference facts generated from the CLI and MCP definitions (FEA-SF-41, FEA-SF-48).
use serde_json::Value;

fn cell(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('|', "&#124;")
        .replace(['\n', '\r'], " ")
}

/// Lists every public command and its flags from clap, including nested commands.
pub fn cli_markdown() -> String {
    fn walk(command: &clap::Command, path: &str, output: &mut String) {
        if command.is_hide_set() || command.get_name() == "help" {
            return;
        }
        output.push_str(&format!(
            "### `{}`\n\n{}\n\n",
            cell(path),
            cell(
                &command
                    .get_about()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            ),
        ));
        let arguments: Vec<_> = command
            .get_arguments()
            .filter(|arg| !arg.is_hide_set())
            .collect();
        if !arguments.is_empty() {
            output.push_str("| Argument | Required | Description |\n| --- | --- | --- |\n");
            for arg in arguments {
                let mut names = Vec::new();
                if let Some(short) = arg.get_short() {
                    names.push(format!("-{short}"));
                }
                if let Some(long) = arg.get_long() {
                    names.push(format!("--{long}"));
                }
                if names.is_empty() {
                    names.push(arg.get_id().to_string());
                }
                let values = arg
                    .get_value_parser()
                    .possible_values()
                    .map(|values| {
                        values
                            .filter(|value| !value.is_hide_set())
                            .map(|value| format!("`{}`", cell(value.get_name())))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|values| !values.is_empty());
                let mut description =
                    cell(&arg.get_help().map(ToString::to_string).unwrap_or_default());
                if let Some(values) = values {
                    description.push_str(&format!(" Values: {values}."));
                }
                if let Some(env) = arg.get_env() {
                    description.push_str(&format!(
                        " Environment: `{}`.",
                        cell(&env.to_string_lossy())
                    ));
                }
                output.push_str(&format!(
                    "| `{}` | {} | {} |\n",
                    cell(&names.join(", ")),
                    if arg.is_required_set() { "yes" } else { "no" },
                    description
                ));
            }
            output.push('\n');
        }
        for child in command.get_subcommands() {
            walk(child, &format!("{path} {}", child.get_name()), output);
        }
    }
    let mut command = crate::cli::definition();
    command.build();
    let mut output = String::new();
    walk(&command, "richview", &mut output);
    output
}

fn schema_type(schema: &Value) -> String {
    if let Some(values) = schema["enum"].as_array() {
        return values
            .iter()
            .map(|v| {
                let value = v
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| v.to_string());
                format!("`{}`", cell(&value))
            })
            .collect::<Vec<_>>()
            .join(", ");
    }
    if schema["type"] == "array" {
        return format!("array of {}", schema_type(&schema["items"]));
    }
    cell(schema["type"].as_str().unwrap_or("value"))
}

/// Lists the tool names, descriptions and input keys clients receive from MCP.
pub fn mcp_markdown() -> String {
    let mut output = String::new();
    for tool in crate::mcp::tool_definitions() {
        let name = tool["name"].as_str().unwrap();
        output.push_str(&format!(
            "### `{}`\n\n{}\n\n",
            cell(name),
            cell(tool["description"].as_str().unwrap_or_default())
        ));
        output
            .push_str("| Argument | Type | Required | Description |\n| --- | --- | --- | --- |\n");
        let schema = &tool["inputSchema"];
        for (name, property) in schema["properties"].as_object().into_iter().flatten() {
            let required = schema["required"]
                .as_array()
                .is_some_and(|keys| keys.iter().any(|key| key == name));
            output.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                cell(name),
                schema_type(property),
                if required { "yes" } else { "no" },
                cell(property["description"].as_str().unwrap_or_default())
            ));
        }
        output.push('\n');
    }
    output
}

/// Summarizes the code's authoring vocabulary and links to the maintained workflow.
pub fn llms_text() -> String {
    let mut output = String::from(
        "# RichView\n\n> A presentation layer for your agent. Write Rich Markdown (.rmd) and render a self-contained report.\n\n## Authoring\n\n- [Authoring skill](https://richview.ai/skill/SKILL.md): workflow and judgment for report authors.\n- [Task guides](https://richview.ai/docs/): local authoring, preview, export and publishing.\n- [Format reference](https://richview.ai/docs/reference.md): fields and values generated from the registry.\n- [CLI, MCP and VS Code](https://richview.ai/docs/tools.md): generated command, flag and tool references.\n\nRun `richview learn`, or call `richview_learn`, for the installed build's authoring guidance. Read the relevant section before choosing fields or values. Publishing is a separate action and requires the user's instruction.\n\n## Rich fences\n\n",
    );
    for tag in richview_core::registry::TAGS {
        output.push_str(&format!("- `{}`: {}\n", tag.name, tag.summary));
    }
    output.push_str("\n## Builtin styles\n\n");
    output.push_str(
        &richview_core::builtin_style_names()
            .iter()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    output.push_str(".\n\n## MCP tools\n\n");
    for tool in crate::mcp::tool_definitions() {
        output.push_str(&format!(
            "- `{}`: {}\n",
            tool["name"].as_str().unwrap(),
            tool["description"].as_str().unwrap_or_default()
        ));
    }
    output
}

/// Expands the factual reference slots in the public tool guide before it is served.
pub fn tools_guide(source: &str) -> Result<String, String> {
    let mut source = source.to_owned();
    for (slot, reference) in [
        ("<!-- richview:cli-reference -->", cli_markdown()),
        ("<!-- richview:mcp-reference -->", mcp_markdown()),
    ] {
        if source.matches(slot).count() != 1 {
            return Err(format!(
                "The tool guide must contain exactly one {slot} slot"
            ));
        }
        source = source.replace(slot, &reference);
    }
    Ok(source)
}

/// Checks shell examples against their command's own flags without opening a report or running a command.
pub fn validate_cli_examples(source: &str) -> Result<(), String> {
    let mut definition = crate::cli::definition();
    definition.build();
    let mut fence = None;
    for line in source.lines() {
        let line = line.trim_start();
        let ticks = line.chars().take_while(|c| *c == '`').count();
        match fence {
            Some((opened, _)) if ticks >= opened && line[ticks..].trim().is_empty() => {
                fence = None;
                continue;
            }
            None if ticks >= 3 => {
                fence = Some((
                    ticks,
                    matches!(line[ticks..].trim(), "sh" | "shell" | "bash"),
                ));
                continue;
            }
            Some((_, true)) => {}
            _ => continue,
        }
        let Some(arguments) = line.trim().strip_prefix("richview ") else {
            continue;
        };
        let mut command = &definition;
        let mut names = vec!["richview"];
        let mut arguments = arguments.split_whitespace().peekable();
        while let Some(word) = arguments.peek().copied() {
            if let Some(child) = command.find_subcommand(word) {
                names.push(word);
                command = child;
                arguments.next();
            } else {
                if !word.starts_with('-')
                    && command.get_subcommands().next().is_some()
                    && !command.get_arguments().any(|arg| arg.get_index().is_some())
                {
                    return Err(format!(
                        "{} has no {word} command in shell example: {line}",
                        names.join(" ")
                    ));
                }
                break;
            }
        }
        for word in arguments {
            if word == "--" {
                break;
            }
            let Some(flag) = word
                .strip_prefix("--")
                .map(|word| word.split('=').next().unwrap())
            else {
                continue;
            };
            if !command
                .get_arguments()
                .any(|arg| arg.get_long() == Some(flag))
            {
                return Err(format!(
                    "{} has no --{flag} flag in shell example: {line}",
                    names.join(" ")
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_served_tool_guide_names_exactly_the_arguments_the_mcp_server_accepts() {
        let source = include_str!("../../../docs/public/tools.md");
        let guide = tools_guide(source).unwrap();
        for tool in crate::mcp::tool_definitions() {
            let name = tool["name"].as_str().unwrap();
            let section = guide
                .split(&format!("### `{name}`\n"))
                .nth(1)
                .unwrap()
                .split("\n### ")
                .next()
                .unwrap();
            let shown: BTreeSet<_> = section
                .lines()
                .filter_map(|line| {
                    line.strip_prefix("| `")?
                        .split_once('`')
                        .map(|(name, _)| name)
                })
                .collect();
            let accepted: BTreeSet<_> = tool["inputSchema"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(shown, accepted, "{name}");
        }
        assert!(
            !guide.contains("input_path"),
            "the examples use the same inputPath as the server"
        );
        assert!(
            !guide.contains("output_path"),
            "the output filename key is outputName"
        );
    }

    #[test]
    fn the_cli_reference_keeps_nested_commands_and_their_own_flags() {
        let reference = cli_markdown();
        let serve = reference
            .split("### `richview serve`\n")
            .nth(1)
            .unwrap()
            .split("\n### ")
            .next()
            .unwrap();
        assert!(serve.contains("--port"));
        assert!(!serve.contains("--format"));
        assert!(reference.contains("### `richview report regenerate`"));
        assert!(!reference.contains("--public-dir"));
        let publish = reference
            .split("### `richview publish`\n")
            .nth(1)
            .unwrap()
            .split("\n### ")
            .next()
            .unwrap();
        assert!(publish.contains("RICHVIEW_TOKEN"));
    }

    #[test]
    fn duplicate_or_missing_reference_slots_fail_instead_of_shipping_a_stale_table() {
        for source in [
            "",
            "<!-- richview:cli-reference -->",
            "<!-- richview:cli-reference --><!-- richview:cli-reference --><!-- richview:mcp-reference -->",
        ] {
            assert!(tools_guide(source).is_err());
        }
    }
    #[test]
    fn shell_flags_are_checked_in_the_command_where_the_example_uses_them() {
        assert!(
            validate_cli_examples("```sh\nrichview serve report.rmd --format json\n```").is_err()
        );
        assert!(validate_cli_examples("```sh\nrichview generate report.rmd --format json\nrichview image search sea --per-page 2\n```").is_ok());
    }
    #[test]
    fn shell_fences_inside_a_quoted_report_do_not_become_command_examples() {
        let source = "````rmd\n```sh\nrichview serve report.rmd --not-a-flag\n```\n````\n";
        assert!(validate_cli_examples(source).is_ok());
    }
}
