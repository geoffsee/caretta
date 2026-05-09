use agent_common::AgentCliAdapter;
use dummy_agent::DummyAgentWrapper;

fn main() {
    let w = DummyAgentWrapper;
    println!("binary           : {}", w.binary());
    println!("help_args        : {:?}", w.help_args());
    println!("version_args     : {:?}", w.version_args());
    println!("model_args       : {:?}", w.model_args("dummy-model"));
    println!("project_args     : {:?}", w.project_args("/tmp"));
    println!("output_format    : {:?}", w.output_format_args("json"));
    println!("yolo_args        : {:?}", w.yolo_args());
    println!("native_run_argv  : {:?}", w.freqai_native_run_argv("hello"));
}
