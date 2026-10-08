use anyhow::Context;
use boardmesh::{DEFAULT_RUNTIME_URL, RUNTIME_METHODS, runtime_request};
use serde_json::Value;
use std::path::PathBuf;

const HELP_TEXT: &str = r#"Usage:
  boardmesh-cli list-methods
  boardmesh-cli <method> [--runtime-url URL] [--json JSON | --json-file PATH] [--pretty|--compact]
  boardmesh-cli call <method> [--runtime-url URL] [--json JSON | --json-file PATH] [--pretty|--compact]

Examples:
  boardmesh-cli list-methods
  boardmesh-cli get_scene
  boardmesh-cli add_text --json '{"position":"120,120","text":"hello"}'
  boardmesh-cli add_text --json '{"session_id":"default","position":"120,120","text":"hello"}'
  boardmesh-cli scene.replace --json '{"session_id":"default","scene":{"objects":[],"selected_object_ids":[]}}'
  boardmesh-cli session.list --compact"#;

#[derive(Debug)]
struct CliOptions {
    method: String,
    runtime_url: String,
    params: Value,
    pretty: bool,
}

fn main() -> anyhow::Result<()> {
    let options = parse_options(std::env::args().skip(1))?;

    if options.method == "list-methods" {
        let output = if options.pretty {
            serde_json::to_string_pretty(&RUNTIME_METHODS)?
        } else {
            serde_json::to_string(&RUNTIME_METHODS)?
        };
        println!("{output}");
        return Ok(());
    }

    if !RUNTIME_METHODS
        .iter()
        .any(|candidate| *candidate == options.method)
    {
        anyhow::bail!(
            "unknown method {}; run `boardmesh-cli list-methods` to inspect the supported command surface",
            options.method
        );
    }

    let result: Value = runtime_request(&options.runtime_url, &options.method, options.params)?;
    let output = if options.pretty {
        serde_json::to_string_pretty(&result)?
    } else {
        serde_json::to_string(&result)?
    };
    println!("{output}");
    Ok(())
}

fn parse_options<I>(args: I) -> anyhow::Result<CliOptions>
where
    I: IntoIterator<Item = String>,
{
    let mut runtime_url = DEFAULT_RUNTIME_URL.to_string();
    let mut pretty = true;
    let mut payload = None;
    let mut payload_path = None;
    let mut positionals = Vec::new();

    let mut args = args.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--runtime-url" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--runtime-url requires a value"))?;
                runtime_url = value;
            }
            "--json" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--json requires a value"))?;
                payload = Some(value);
            }
            "--json-file" => {
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--json-file requires a value"))?;
                payload_path = Some(PathBuf::from(value));
            }
            "--compact" => pretty = false,
            "--pretty" => pretty = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => positionals.push(argument),
        }
    }

    let method = if positionals.is_empty() {
        print_help();
        std::process::exit(1);
    } else if positionals.len() == 1 {
        positionals[0].clone()
    } else if positionals.len() == 2 && positionals[0] == "call" {
        positionals[1].clone()
    } else {
        anyhow::bail!("expected `boardmesh-cli <method>` or `boardmesh-cli call <method>`");
    };

    let params = if let Some(path) = payload_path {
        serde_json::from_str::<Value>(
            &std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read json payload from {}", path.display()))?,
        )
        .with_context(|| format!("invalid json payload in {}", path.display()))?
    } else if let Some(payload) = payload {
        serde_json::from_str::<Value>(&payload).context("invalid --json payload")?
    } else {
        Value::Object(Default::default())
    };

    Ok(CliOptions {
        method,
        runtime_url,
        params,
        pretty,
    })
}

fn print_help() {
    eprintln!("{HELP_TEXT}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    fn parse(args: &[&str]) -> anyhow::Result<CliOptions> {
        parse_options(args.iter().map(|arg| (*arg).to_string()))
    }

    #[test]
    fn parses_direct_method_with_inline_json() {
        let options = parse(&[
            "add_text",
            "--runtime-url",
            "ws://127.0.0.1:9999",
            "--json",
            r#"{"position":"120,120","text":"hello"}"#,
            "--compact",
        ])
        .expect("parse options");

        assert_eq!(options.method, "add_text");
        assert_eq!(options.runtime_url, "ws://127.0.0.1:9999");
        assert!(!options.pretty);
        assert_eq!(options.params["text"], "hello");
    }

    #[test]
    fn parses_call_alias_and_json_file() {
        let temp_path = std::env::temp_dir().join(format!("boardmesh-cli-{}.json", Uuid::new_v4()));
        fs::write(
            &temp_path,
            r#"{"session_id":"default","position":"10,10","text":"file"}"#,
        )
        .expect("write temp payload");

        let options = parse(&[
            "call",
            "add_text",
            "--json-file",
            temp_path.to_str().expect("path string"),
        ])
        .expect("parse options");

        assert_eq!(options.method, "add_text");
        assert!(options.pretty);
        assert_eq!(options.params["session_id"], "default");
        assert_eq!(options.params["text"], "file");

        let _ = fs::remove_file(temp_path);
    }

    #[test]
    fn defaults_to_empty_object_payload() {
        let options = parse(&["session.list"]).expect("parse options");

        assert_eq!(options.method, "session.list");
        assert_eq!(options.params, Value::Object(Default::default()));
        assert!(options.pretty);
    }

    #[test]
    fn rejects_invalid_positional_shape() {
        let error = parse(&["call", "add_text", "extra"]).expect_err("expected parse error");
        assert!(
            error
                .to_string()
                .contains("expected `boardmesh-cli <method>` or `boardmesh-cli call <method>`")
        );
    }

    #[test]
    fn help_text_uses_boardmesh_cli_name() {
        assert!(HELP_TEXT.contains("boardmesh-cli list-methods"));
        assert!(!HELP_TEXT.contains("whiteboard-cli"));
    }
}
