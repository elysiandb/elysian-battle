use anyhow::Result;
use clap::{Parser, ValueEnum};
use dialoguer::{Input, Select};

/// Comprehensive integration test harness for ElysianDB.
#[derive(Parser, Debug)]
#[command(name = "elysian-battle", version, about, disable_version_flag = true)]
pub struct Cli {
    /// Git ref to test: branch name, tag, or "latest"
    #[arg(long = "version", value_name = "REF")]
    pub ref_version: Option<String>,

    /// Comma-separated list of test suites to run
    #[arg(long)]
    pub suite: Option<String>,

    /// Output report format
    #[arg(long, default_value = "text")]
    pub report: ReportFormat,

    /// Skip Go compilation, reuse last binary
    #[arg(long)]
    pub no_build: bool,

    /// Keep ElysianDB running after tests complete
    #[arg(long)]
    pub keep_alive: bool,

    /// Enable detailed logging
    #[arg(long)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReportFormat {
    Text,
    Json,
}

impl Cli {
    /// Parse the suite filter into a list of suite names.
    pub fn parse_suites(&self) -> Option<Vec<String>> {
        self.suite.as_ref().map(|s| {
            s.split(',')
                .map(|name| name.trim().to_lowercase())
                .filter(|name| !name.is_empty())
                .collect()
        })
    }

    /// If `--version` was not provided, interactively prompt the user
    /// to select a version source and enter the ref name.
    /// Returns the resolved git ref string.
    pub fn resolve_version_interactive(&self) -> Result<String> {
        if let Some(ref v) = self.ref_version {
            return Ok(v.clone());
        }

        let sources = &["branch", "tag", "latest"];
        let selection = Select::new()
            .with_prompt("Select version source")
            .items(sources)
            .default(0)
            .interact()?;

        let ref_name = match sources[selection] {
            "latest" => "latest".to_string(),
            kind => {
                let prompt = format!("Enter {} name", kind);
                Input::<String>::new()
                    .with_prompt(prompt)
                    .default("main".into())
                    .interact_text()?
            }
        };

        Ok(ref_name)
    }
}
