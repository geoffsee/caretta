use agent_common::AgentCliAdapter;
use dummy_agent::DummyAgentWrapper;
use std::process::Command;

fn main() {
    let w = DummyAgentWrapper;
    for args in [w.help_args(), w.version_args()] {
        let status = Command::new(w.binary())
            .args(&args)
            .status()
            .expect("failed to spawn freq-ai-dummy-agent — is it on PATH?");
        assert!(
            status.success(),
            "{} {:?} exited with {:?}",
            w.binary(),
            args,
            status.code()
        );
        println!("ok: {} {:?}", w.binary(), args);
    }

    let probe = w.freqai_native_run_argv("probe");
    let status = Command::new(w.binary())
        .args(&probe)
        .status()
        .expect("failed to spawn freq-ai-dummy-agent");
    assert!(
        status.success(),
        "{} {:?} exited with {:?}",
        w.binary(),
        probe,
        status.code()
    );
    println!("ok: {} {:?}", w.binary(), probe);
}
