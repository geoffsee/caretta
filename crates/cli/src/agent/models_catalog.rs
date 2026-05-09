use crate::agent::types::AgentExt;
use clap::ValueEnum;
use std::collections::BTreeSet;

use cli_common::Agent;

/// Print bundled model entries from `assets/available-models.json` (via
/// [`AgentExt::available_models`]). `--plain` is intended for shell completion.
pub fn run_models_list(selected: Agent, plain: bool, all: bool) {
    let agents: Vec<Agent> = if all {
        Agent::value_variants().to_vec()
    } else {
        vec![selected]
    };

    if all && plain {
        let mut ids = BTreeSet::new();
        for agent in &agents {
            for (id, _) in agent.available_models() {
                ids.insert(*id);
            }
        }
        for id in ids {
            println!("{id}");
        }
        return;
    }

    if all && !plain {
        for agent in agents {
            print_agent_section(agent);
        }
        return;
    }

    let models = selected.available_models();
    if models.is_empty() {
        eprintln!(
            "No bundled models for {selected}. Rebuild `freq-ai-agent-runtime` (bundled CLIs in node_modules) and then the CLI, or pick any ID your adapter accepts."
        );
        return;
    }

    if plain {
        for (id, _) in models {
            println!("{id}");
        }
        return;
    }

    let id_w = models.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
    println!("Bundled models for {selected} (regenerate: cargo build -p freq-ai-agent-runtime)\n");
    for (id, label) in models {
        println!("{id:id_w$}  {label}", id_w = id_w);
    }
}

fn print_agent_section(agent: Agent) {
    let models = agent.available_models();

    println!("[{agent}]");
    if models.is_empty() {
        println!("  (none bundled)");
        println!();
        return;
    }
    let id_w = models.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
    for (id, label) in models {
        println!("  {id:id_w$}  {label}", id_w = id_w);
    }
    println!();
}
